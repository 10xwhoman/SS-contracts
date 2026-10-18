//! Tests for the invoice maturity contract (issue #536).

#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::{ContractEvent, ContractEventBody, ScAddress, ScVal},
    Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

const START: u64 = 1_000;
const ONE_DAY: u64 = 86_400;

/// Mock downstream settlement contract recording hand-off invocations.
#[contract]
pub struct MockSettlement;

#[contractimpl]
impl MockSettlement {
    pub fn on_maturity(env: Env, invoice_id: Symbol) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "last"), &invoice_id);
    }

    pub fn last_invoice(env: Env) -> Option<Symbol> {
        env.storage().instance().get(&Symbol::new(&env, "last"))
    }
}

struct Ctx<'a> {
    env: Env,
    admin: Address,
    maturity: InvoiceMaturityClient<'a>,
    invoice_id: Symbol,
}

fn setup<'a>(env: &'a Env) -> Ctx<'a> {
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(env);
    let contract_id = env.register(InvoiceMaturity, ());
    let maturity = InvoiceMaturityClient::new(env, &contract_id);
    maturity.initialize(&admin);

    Ctx {
        env: env.clone(),
        admin,
        maturity,
        invoice_id: Symbol::new(env, "INVMAT01"),
    }
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

// ── set_maturity ──────────────────────────────────────────────────────────────

#[test]
fn set_maturity_records_timestamp_and_days_remaining() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + 3 * ONE_DAY + 43_200; // 3.5 days out

    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    let info = ctx.maturity.get_maturity_info(&ctx.invoice_id);
    assert_eq!(info.maturity_timestamp, maturity_ts);
    assert_eq!(info.days_remaining, 3);
    assert!(!info.is_mature);
    assert!(!info.settlement_triggered);

    let record = ctx.maturity.get_maturity(&ctx.invoice_id);
    assert_eq!(record.maturity_timestamp, maturity_ts);
    assert_eq!(record.set_at, START);
    assert!(!ctx.maturity.is_mature(&ctx.invoice_id));
}

#[test]
fn set_maturity_admin_only() {
    let env = Env::default();
    let ctx = setup(&env);
    let impostor = Address::generate(&env);
    let result = ctx
        .maturity
        .try_set_maturity(&impostor, &ctx.invoice_id, &(START + ONE_DAY));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn set_maturity_rejects_past_timestamp_and_duplicates() {
    let env = Env::default();
    let ctx = setup(&env);

    let result = ctx
        .maturity
        .try_set_maturity(&ctx.admin, &ctx.invoice_id, &START);
    assert_eq!(result, Err(Ok(Error::InvalidMaturity)));

    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &(START + ONE_DAY));
    let result = ctx
        .maturity
        .try_set_maturity(&ctx.admin, &ctx.invoice_id, &(START + 2 * ONE_DAY));
    assert_eq!(result, Err(Ok(Error::MaturityAlreadySet)));
}

#[test]
fn days_remaining_floors_partial_days() {
    let env = Env::default();
    let ctx = setup(&env);
    // 1 day + 23 hours + 59 minutes: still 1 whole day remaining.
    let maturity_ts = START + ONE_DAY + 86_340;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    let info = ctx.maturity.get_maturity_info(&ctx.invoice_id);
    assert_eq!(info.days_remaining, 1);

    // Under one day remaining floors to 0 but is not yet mature.
    ctx.env.ledger().set_timestamp(maturity_ts - 10);
    let info = ctx.maturity.get_maturity_info(&ctx.invoice_id);
    assert_eq!(info.days_remaining, 0);
    assert!(!info.is_mature);
}

// ── is_mature ─────────────────────────────────────────────────────────────────

#[test]
fn is_mature_tracks_block_timestamp() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    assert!(!ctx.maturity.is_mature(&ctx.invoice_id));

    ctx.env.ledger().set_timestamp(maturity_ts - 1);
    assert!(!ctx.maturity.is_mature(&ctx.invoice_id));

    // Exact maturity timestamp counts as mature.
    ctx.env.ledger().set_timestamp(maturity_ts);
    assert!(ctx.maturity.is_mature(&ctx.invoice_id));

    ctx.env.ledger().set_timestamp(maturity_ts + 5_000);
    assert!(ctx.maturity.is_mature(&ctx.invoice_id));
}

#[test]
fn is_mature_unknown_invoice_is_false() {
    let env = Env::default();
    let ctx = setup(&env);
    assert!(!ctx.maturity.is_mature(&Symbol::new(&env, "GHOST")));
}

// ── trigger_maturity ──────────────────────────────────────────────────────────

#[test]
fn trigger_maturity_before_maturity_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    let result = ctx.maturity.try_trigger_maturity(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::NotMature)));

    ctx.env.ledger().set_timestamp(maturity_ts - 1);
    let result = ctx.maturity.try_trigger_maturity(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::NotMature)));
}

#[test]
fn trigger_maturity_at_maturity_initiates_settlement() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    // A non-admin "cron" caller may trigger once mature (permissionless).
    ctx.env.ledger().set_timestamp(maturity_ts);
    ctx.maturity.trigger_maturity(&ctx.invoice_id);

    let record = ctx.maturity.get_maturity(&ctx.invoice_id);
    assert!(record.settlement_triggered);
    assert_eq!(record.triggered_at, maturity_ts);

    let info = ctx.maturity.get_maturity_info(&ctx.invoice_id);
    assert!(info.is_mature);
    assert!(info.settlement_triggered);
    assert_eq!(info.days_remaining, 0);
}

#[test]
fn trigger_maturity_twice_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    ctx.env.ledger().set_timestamp(maturity_ts + 1);
    ctx.maturity.trigger_maturity(&ctx.invoice_id);

    let result = ctx.maturity.try_trigger_maturity(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(Error::SettlementAlreadyTriggered)));
}

#[test]
fn trigger_maturity_unknown_invoice_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let result = ctx
        .maturity
        .try_trigger_maturity(&Symbol::new(&env, "GHOST"));
    assert_eq!(result, Err(Ok(Error::MaturityNotFound)));
}

// ── Events ────────────────────────────────────────────────────────────────────

#[test]
fn maturity_reached_emitted_with_invoice_and_timestamp() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + 2 * ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);
    ctx.env.ledger().set_timestamp(maturity_ts + 3);
    ctx.maturity.trigger_maturity(&ctx.invoice_id);

    let mut found = false;
    let mut triggered_event = false;
    for e in env.events().all().events().iter() {
        let topic = topic_of(&env, e);
        if topic == Symbol::new(&env, "maturity_reached") {
            found = true;
            let (_, _, data) = parse_event(&env, e);
            let (inv_id, ts): (Symbol, u64) = data.try_into_val(&env).unwrap();
            assert_eq!(inv_id, ctx.invoice_id);
            assert_eq!(ts, maturity_ts);
        }
        if topic == Symbol::new(&env, "settlement_triggered") {
            triggered_event = true;
            let (_, _, data) = parse_event(&env, e);
            let (inv_id, at): (Symbol, u64) = data.try_into_val(&env).unwrap();
            assert_eq!(inv_id, ctx.invoice_id);
            assert_eq!(at, maturity_ts + 3);
        }
    }
    assert!(found, "expected MaturityReached event");
    assert!(triggered_event, "expected SettlementTriggered event");
}

#[test]
fn maturity_set_event_emitted() {
    let env = Env::default();
    let ctx = setup(&env);
    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);

    let mut found = false;
    for e in env.events().all().events().iter() {
        if topic_of(&env, e) == Symbol::new(&env, "maturity_set") {
            found = true;
            let (_, _, data) = parse_event(&env, e);
            let (inv_id, ts): (Symbol, u64) = data.try_into_val(&env).unwrap();
            assert_eq!(inv_id, ctx.invoice_id);
            assert_eq!(ts, maturity_ts);
        }
    }
    assert!(found, "expected MaturitySet event");
}

// ── Settlement hand-off ───────────────────────────────────────────────────────

#[test]
fn trigger_maturity_invokes_settlement_contract_hook() {
    let env = Env::default();
    let ctx = setup(&env);

    let settlement_id = env.register(MockSettlement, ());
    let settlement = MockSettlementClient::new(&env, &settlement_id);
    ctx.maturity
        .set_settlement_contract(&ctx.admin, &Some(settlement_id.clone()));

    let maturity_ts = START + ONE_DAY;
    ctx.maturity
        .set_maturity(&ctx.admin, &ctx.invoice_id, &maturity_ts);
    ctx.env.ledger().set_timestamp(maturity_ts);
    ctx.maturity.trigger_maturity(&ctx.invoice_id);

    assert_eq!(settlement.last_invoice(), Some(ctx.invoice_id.clone()));
}

#[test]
fn set_settlement_contract_admin_only_and_clearable() {
    let env = Env::default();
    let ctx = setup(&env);
    let impostor = Address::generate(&env);
    let settlement_id = env.register(MockSettlement, ());

    let result = ctx
        .maturity
        .try_set_settlement_contract(&impostor, &Some(settlement_id.clone()));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    ctx.maturity
        .set_settlement_contract(&ctx.admin, &Some(settlement_id));
    assert!(ctx.maturity.get_config().settlement_contract.is_some());

    ctx.maturity.set_settlement_contract(&ctx.admin, &None);
    assert!(ctx.maturity.get_config().settlement_contract.is_none());
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let result = ctx.maturity.try_initialize(&ctx.admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}
