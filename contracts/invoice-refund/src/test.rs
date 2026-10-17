//! Tests for the invoice refund contract (issue #525).

#![allow(deprecated)]

use super::*;
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::{ContractEvent, ContractEventBody, ScAddress, ScVal},
    Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

const THRESHOLD: i128 = 2_000;
const EXPIRY: u64 = 10_000;
const START: u64 = 1_000;

struct Ctx<'a> {
    env: Env,
    admin: Address,
    token: TokenClient<'a>,
    asset: AssetClient<'a>,
    contract_id: Address,
    refund: InvoiceRefundClient<'a>,
    invoice_id: Symbol,
}

/// Stand up a fresh contract + USDC-like Stellar asset token and register one
/// invoice with `THRESHOLD` / `EXPIRY`.
fn setup<'a>(env: &'a Env) -> Ctx<'a> {
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(env, &token_contract.address());
    let asset = AssetClient::new(env, &token_contract.address());

    let contract_id = env.register(InvoiceRefund, ());
    let refund = InvoiceRefundClient::new(env, &contract_id);
    refund.initialize(&admin, &token_contract.address());

    let invoice_id = Symbol::new(env, "INVREF01");
    refund.register_invoice(&admin, &invoice_id, &THRESHOLD, &EXPIRY);

    Ctx {
        env: env.clone(),
        admin,
        token,
        asset,
        contract_id,
        refund,
        invoice_id,
    }
}

/// Mint USDC to an investor and lock it into the invoice.
fn invest(ctx: &Ctx<'_>, investor: &Address, amount: i128) {
    ctx.asset.mint(investor, &amount);
    ctx.refund
        .record_investment(&ctx.invoice_id, investor, &amount);
}

fn parse_event(env: &Env, event: &ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => {
            Address::try_from_val(env, &ScVal::Address(ScAddress::Contract(hash.clone()))).unwrap()
        }
        None => Address::generate(env),
    };
    let ContractEventBody::V0(v0) = &event.body;
    let topics =
        Vec::<Val>::try_from_val(env, &ScVal::Vec(Some(v0.topics.clone().into()))).unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

fn topic_of(env: &Env, event: &ContractEvent) -> Symbol {
    let (_, topics, _) = parse_event(env, event);
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

/// Find the amount reported for `investor` in a `refund_processed` event.
fn refund_amount_for(env: &Env, investor: &Address) -> Option<i128> {
    for e in env.events().all().events().iter() {
        if topic_of(env, e) == Symbol::new(env, "refund_processed") {
            let (_, _, data) = parse_event(env, e);
            let (_inv, addr, amount): (Symbol, Address, i128) = data.try_into_val(env).unwrap();
            if addr == *investor {
                return Some(amount);
            }
        }
    }
    None
}

// ── Expiry validation ─────────────────────────────────────────────────────────

#[test]
fn trigger_refund_before_expiry_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    invest(&ctx, &investor, 500);

    let result = ctx.refund.try_trigger_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::InvoiceNotExpired)));

    // Still rejected one second before expiry.
    ctx.env.ledger().set_timestamp(EXPIRY - 1);
    let result = ctx.refund.try_trigger_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::InvoiceNotExpired)));
}

#[test]
fn trigger_refund_succeeds_at_exact_expiry() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    invest(&ctx, &investor, 500);

    ctx.env.ledger().set_timestamp(EXPIRY);
    let summary = ctx.refund.trigger_refund(&ctx.invoice_id);
    assert_eq!(summary.total_refunded, 500);
    assert_eq!(ctx.token.balance(&investor), 500);
}

// ── Pro-rata refund correctness ───────────────────────────────────────────────

#[test]
fn trigger_refund_returns_correct_pro_rata_amounts() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    // Total 1000 < 2000 threshold: the invoice expires under-funded.
    invest(&ctx, &alice, 500);
    invest(&ctx, &bob, 300);
    invest(&ctx, &carol, 200);
    assert_eq!(ctx.token.balance(&ctx.contract_id), 1_000);

    ctx.env.ledger().set_timestamp(EXPIRY + 42);
    let summary = ctx.refund.trigger_refund(&ctx.invoice_id);

    assert_eq!(summary.invoice_id, ctx.invoice_id);
    assert_eq!(summary.total_refunded, 1_000);
    assert_eq!(summary.investor_count, 3);
    assert_eq!(summary.refunded_at, EXPIRY + 42);

    // Each investor receives back exactly their locked share.
    assert_eq!(ctx.token.balance(&alice), 500);
    assert_eq!(ctx.token.balance(&bob), 300);
    assert_eq!(ctx.token.balance(&carol), 200);
    // No USDC is left stranded in the contract.
    assert_eq!(ctx.token.balance(&ctx.contract_id), 0);
    // Positions are cleared.
    assert_eq!(ctx.refund.get_investment(&ctx.invoice_id, &alice), 0);
    assert_eq!(ctx.refund.get_investment(&ctx.invoice_id, &bob), 0);
    assert_eq!(ctx.refund.get_investment(&ctx.invoice_id, &carol), 0);
}

#[test]
fn repeat_investment_from_same_investor_accumulates() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);

    invest(&ctx, &alice, 400);
    invest(&ctx, &alice, 350);
    assert_eq!(ctx.refund.get_investment(&ctx.invoice_id, &alice), 750);

    ctx.env.ledger().set_timestamp(EXPIRY + 1);
    let summary = ctx.refund.trigger_refund(&ctx.invoice_id);
    assert_eq!(summary.investor_count, 1);
    assert_eq!(ctx.token.balance(&alice), 750);
}

// ── Event emission ────────────────────────────────────────────────────────────

#[test]
fn refund_processed_emitted_per_investor_with_amount() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    invest(&ctx, &alice, 600);
    invest(&ctx, &bob, 400);

    ctx.env.ledger().set_timestamp(EXPIRY + 5);
    ctx.refund.trigger_refund(&ctx.invoice_id);

    let mut processed_events = 0u32;
    for e in env.events().all().events().iter() {
        if topic_of(&env, e) == Symbol::new(&env, "refund_processed") {
            processed_events += 1;
            let (_, _, data) = parse_event(&env, e);
            let (inv_id, _investor, amount): (Symbol, Address, i128) =
                data.try_into_val(&env).unwrap();
            assert_eq!(inv_id, ctx.invoice_id);
            assert!(amount == 600 || amount == 400);
        }
    }
    assert_eq!(processed_events, 2);
    assert_eq!(refund_amount_for(&env, &alice), Some(600));
    assert_eq!(refund_amount_for(&env, &bob), Some(400));
}

#[test]
fn invoice_refund_complete_emitted_after_all_refunds() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    invest(&ctx, &alice, 700);
    invest(&ctx, &bob, 300);

    ctx.env.ledger().set_timestamp(EXPIRY + 5);
    ctx.refund.trigger_refund(&ctx.invoice_id);

    let mut last_processed_index: i32 = -1;
    let mut complete_index: i32 = -1;
    for (index, e) in env.events().all().events().iter().enumerate() {
        let topic = topic_of(&env, e);
        if topic == Symbol::new(&env, "refund_processed") {
            last_processed_index = index as i32;
        }
        if topic == Symbol::new(&env, "invoice_refund_complete") {
            complete_index = index as i32;
            let (_, _, data) = parse_event(&env, e);
            let (inv_id, total, count): (Symbol, i128, u32) = data.try_into_val(&env).unwrap();
            assert_eq!(inv_id, ctx.invoice_id);
            assert_eq!(total, 1_000);
            assert_eq!(count, 2);
        }
    }
    assert!(last_processed_index >= 0, "no refund_processed events");
    assert!(complete_index >= 0, "no invoice_refund_complete event");
    assert!(
        complete_index > last_processed_index,
        "InvoiceRefundComplete must be emitted after every RefundProcessed"
    );
}

// ── Guard rails ───────────────────────────────────────────────────────────────

#[test]
fn re_trigger_on_refunded_invoice_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    invest(&ctx, &investor, 500);

    ctx.env.ledger().set_timestamp(EXPIRY + 10);
    ctx.refund.trigger_refund(&ctx.invoice_id);

    let result = ctx.refund.try_trigger_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::AlreadyRefunded)));
}

#[test]
fn funded_invoice_is_not_refundable() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    invest(&ctx, &investor, THRESHOLD);

    ctx.env.ledger().set_timestamp(EXPIRY + 10);
    let result = ctx.refund.try_trigger_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::InvoiceFunded)));
    // Funds stay with the contract for the settlement path.
    assert_eq!(ctx.token.balance(&ctx.contract_id), THRESHOLD);
    assert!(!ctx.refund.is_refundable(&ctx.invoice_id));
}

#[test]
fn investment_after_expiry_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    ctx.asset.mint(&investor, &500);

    ctx.env.ledger().set_timestamp(EXPIRY);
    let result = ctx
        .refund
        .try_record_investment(&ctx.invoice_id, &investor, &500);
    assert_eq!(result, Err(Ok(Error::InvoiceExpired)));
}

#[test]
fn investment_beyond_funding_threshold_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    invest(&ctx, &alice, 1_500);

    ctx.asset.mint(&bob, &600);
    let result = ctx
        .refund
        .try_record_investment(&ctx.invoice_id, &bob, &600);
    assert_eq!(result, Err(Ok(Error::ExceedsFundingThreshold)));
    assert_eq!(ctx.token.balance(&ctx.contract_id), 1_500);
}

#[test]
fn zero_amount_investment_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    let result = ctx
        .refund
        .try_record_investment(&ctx.invoice_id, &investor, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn trigger_refund_unknown_invoice_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let unknown = Symbol::new(&env, "NOPE");
    let result = ctx.refund.try_trigger_refund(&unknown);
    assert_eq!(result, Err(Ok(Error::InvoiceNotFound)));
}

// ── Registration validation ───────────────────────────────────────────────────

#[test]
fn register_invoice_rejects_non_admin() {
    let env = Env::default();
    let ctx = setup(&env);
    let impostor = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INVX");
    let result = ctx
        .refund
        .try_register_invoice(&impostor, &invoice_id, &1_000, &(EXPIRY + 1_000));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn register_invoice_rejects_past_expiry_and_duplicates() {
    let env = Env::default();
    let ctx = setup(&env);

    let past = Symbol::new(&env, "INVPAST");
    let result = ctx
        .refund
        .try_register_invoice(&ctx.admin, &past, &1_000, &START);
    assert_eq!(result, Err(Ok(Error::InvalidExpiry)));

    let result =
        ctx.refund
            .try_register_invoice(&ctx.admin, &ctx.invoice_id, &1_000, &(EXPIRY + 1));
    assert_eq!(result, Err(Ok(Error::InvoiceAlreadyExists)));

    let zero = Symbol::new(&env, "INVZERO");
    let result = ctx
        .refund
        .try_register_invoice(&ctx.admin, &zero, &0, &(EXPIRY + 1));
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let result = ctx.refund.try_initialize(&ctx.admin, &ctx.token.address);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

// ── Views ─────────────────────────────────────────────────────────────────────

#[test]
fn views_track_expiry_and_refundability() {
    let env = Env::default();
    let ctx = setup(&env);
    let investor = Address::generate(&env);
    invest(&ctx, &investor, 100);

    assert!(!ctx.refund.is_expired(&ctx.invoice_id));
    assert!(!ctx.refund.is_refundable(&ctx.invoice_id));

    ctx.env.ledger().set_timestamp(EXPIRY);
    assert!(ctx.refund.is_expired(&ctx.invoice_id));
    assert!(ctx.refund.is_refundable(&ctx.invoice_id));

    ctx.refund.trigger_refund(&ctx.invoice_id);
    assert!(!ctx.refund.is_refundable(&ctx.invoice_id));

    let record = ctx.refund.get_invoice(&ctx.invoice_id);
    assert!(record.refunded);
    assert_eq!(record.total_locked, 100);
    assert_eq!(record.investors.len(), 1);

    let config = ctx.refund.get_config();
    assert_eq!(config.admin, ctx.admin);
    assert_eq!(config.usdc_token, ctx.token.address);
}
