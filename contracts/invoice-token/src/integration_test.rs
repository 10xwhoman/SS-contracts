#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]
//! #382 — cross-contract integration tests for the escrow-triggered
//! burn-on-settlement callback.
//!
//! `MockSettlementEscrow` stands in for the real `invoice-escrow` contract:
//! it holds no funds and runs no lifecycle logic of its own, it only calls
//! `burn`/`burn_from` on the configured `InvoiceToken` the same way the real
//! escrow's `record_payment`/`refund_escrow` paths do, so these tests can
//! assert on cross-contract authorization, allowance consumption, and
//! post-settlement transfer unlock without pulling in the entire
//! invoice-escrow contract as a dependency of this crate.

use super::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal,
    Val, Vec,
};

use crate::errors::Error;

#[contract]
struct MockSettlementEscrow;

#[contractimpl]
impl MockSettlementEscrow {
    /// Mirrors `invoice-escrow::record_payment`'s full settlement path:
    /// burns the investor's token position directly (no allowance needed --
    /// the escrow holds the tokens itself in the real flow) and, once
    /// settlement completes, unlocks transfers on the token contract.
    pub fn burn_on_settlement(
        env: Env,
        token: Address,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        client.burn(&investor, &amount);
        client.set_transfer_locked(&env.current_contract_address(), &false);
        Ok(())
    }

    /// Mirrors a settlement path where the escrow only holds an allowance
    /// over the investor's tokens (e.g. a secondary-market position that
    /// never transferred custody to the escrow), via `burn_from`.
    pub fn burn_from_on_settlement(
        env: Env,
        token: Address,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        client.burn_from(&env.current_contract_address(), &investor, &amount);
        client.set_transfer_locked(&env.current_contract_address(), &false);
        Ok(())
    }
}

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

/// Returns the topic-1 Symbol of the most recent event `contract_id` emitted, if any.
fn last_event_topic(env: &Env, contract_id: &Address) -> Option<Symbol> {
    let all = env.events().all();
    let raw_events = all.events();
    raw_events
        .iter()
        .rev()
        .map(|e| parse_event(env, e))
        .find(|(addr, _, _)| addr == contract_id)
        .and_then(|(_, topics, _)| topics.get(0))
        .and_then(|v| Symbol::try_from_val(env, &v).ok())
}

/// Whether any event `contract_id` emitted has `topic` as its first topic.
fn has_event_with_topic(env: &Env, contract_id: &Address, topic: &Symbol) -> bool {
    let all = env.events().all();
    let raw_events = all.events();
    raw_events.iter().any(|e| {
        let (addr, topics, _) = parse_event(env, e);
        &addr == contract_id
            && topics
                .get(0)
                .and_then(|v| Symbol::try_from_val(env, &v).ok())
                .as_ref()
                == Some(topic)
    })
}

struct Harness {
    env: Env,
    token: InvoiceTokenClient<'static>,
    token_id: Address,
    escrow_id: Address,
    admin: Address,
    investor: Address,
}

impl Harness {
    /// `initial_balance` is minted to `investor` by `admin` up front (the
    /// escrow itself never mints in the real flow -- `create_escrow`/
    /// `fund_escrow` do, via a separate call this harness doesn't need to
    /// model to exercise the burn callback).
    fn new(initial_balance: i128) -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let token_id = env.register(InvoiceToken, ());
        let token = InvoiceTokenClient::new(&env, &token_id);
        let escrow_id = env.register(MockSettlementEscrow, ());

        let admin = Address::generate(&env);
        let investor = Address::generate(&env);
        let name = SorobanString::from_str(&env, "Invoice INV-SETL");
        let symbol = SorobanString::from_str(&env, "INVSETL");
        let invoice_id = Symbol::new(&env, "inv_setl");

        // The escrow is the minter, matching how invoice-escrow's
        // create_escrow deploys/owns the invoice token in production.
        token.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &escrow_id);
        if initial_balance > 0 {
            token.mint(&investor, &initial_balance, &escrow_id);
        }

        let token = unsafe { core::mem::transmute::<_, InvoiceTokenClient<'static>>(token) };

        Harness {
            env,
            token,
            token_id: token_id.clone(),
            escrow_id,
            admin,
            investor,
        }
    }

    fn escrow_client(&self) -> MockSettlementEscrowClient<'_> {
        MockSettlementEscrowClient::new(&self.env, &self.escrow_id)
    }
}

// ── Happy path ──────────────────────────────────────────────────────────

#[test]
fn burn_on_settlement_retires_the_investors_full_position() {
    let h = Harness::new(1_000);

    h.escrow_client()
        .burn_on_settlement(&h.token_id, &h.investor, &1_000);

    assert_eq!(h.token.balance(&h.investor), 0);
    assert_eq!(h.token.total_supply(), 0);
}

#[test]
fn burn_on_settlement_emits_the_underlying_burn_event() {
    let h = Harness::new(500);
    h.escrow_client()
        .burn_on_settlement(&h.token_id, &h.investor, &500);

    let topic = last_event_topic(&h.env, &h.token_id);
    // set_transfer_locked runs after burn, so the *last* token event is its
    // own transfer_locked_updated event; assert burn happened by checking
    // the event stream contains a burn topic at all.
    let burn_seen = has_event_with_topic(&h.env, &h.token_id, &Symbol::new(&h.env, "burn"));
    assert!(burn_seen, "expected a burn event from the token contract");
    assert_eq!(topic, Some(Symbol::new(&h.env, "transfer_locked_updated")));
}

#[test]
fn burn_from_on_settlement_happy_path_consumes_allowance_and_burns() {
    let h = Harness::new(1_000);
    h.token.approve(&h.investor, &h.escrow_id, &1_000, &1_000);

    h.escrow_client()
        .burn_from_on_settlement(&h.token_id, &h.investor, &600);

    assert_eq!(h.token.balance(&h.investor), 400);
    assert_eq!(h.token.total_supply(), 400);
}

#[test]
fn burn_from_deducts_exactly_the_burned_amount_from_the_allowance() {
    let h = Harness::new(1_000);
    h.token.approve(&h.investor, &h.escrow_id, &1_000, &1_000);

    h.token.burn_from(&h.escrow_id, &h.investor, &300);

    assert_eq!(h.token.allowance(&h.investor, &h.escrow_id), 700);
    assert_eq!(h.token.balance(&h.investor), 700);
}

// ── Post-settlement transfer unlock ───────────────────────────────────────

#[test]
fn transfers_are_locked_before_settlement_and_unlocked_after() {
    let h = Harness::new(1_000);
    let recipient = Address::generate(&h.env);

    // Locked by default (initialize() sets transfer_locked: true) -- a
    // non-admin, non-minter transfer is rejected.
    let before = h.token.try_transfer(&h.investor, &recipient, &100);
    assert_eq!(before, Err(Ok(Error::TransferLocked)));

    h.escrow_client()
        .burn_on_settlement(&h.token_id, &h.investor, &400);
    assert!(!h.token.transfer_locked());

    // Now unlocked: the investor can move their residual balance.
    h.token.transfer(&h.investor, &recipient, &100);
    assert_eq!(h.token.balance(&recipient), 100);
}

#[test]
fn residual_balance_persists_accurately_after_partial_settlement() {
    let h = Harness::new(1_000);

    h.escrow_client()
        .burn_on_settlement(&h.token_id, &h.investor, &350);

    assert_eq!(h.token.balance(&h.investor), 650);
    assert_eq!(h.token.total_supply(), 650);

    // A second, independent settlement call against the remaining position.
    h.escrow_client()
        .burn_on_settlement(&h.token_id, &h.investor, &650);
    assert_eq!(h.token.balance(&h.investor), 0);
    assert_eq!(h.token.total_supply(), 0);
}

// ── Negative paths ─────────────────────────────────────────────────────

#[test]
fn burn_on_settlement_rejects_insufficient_balance() {
    let h = Harness::new(100);

    let result = h
        .escrow_client()
        .try_burn_on_settlement(&h.token_id, &h.investor, &101);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
    // Balance is unaffected by the failed attempt.
    assert_eq!(h.token.balance(&h.investor), 100);
}

#[test]
fn burn_on_settlement_rejects_invalid_amount() {
    let h = Harness::new(100);

    let result = h
        .escrow_client()
        .try_burn_on_settlement(&h.token_id, &h.investor, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));

    let result = h
        .escrow_client()
        .try_burn_on_settlement(&h.token_id, &h.investor, &-1);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn burn_from_on_settlement_rejects_insufficient_allowance() {
    let h = Harness::new(1_000);
    h.token.approve(&h.investor, &h.escrow_id, &100, &1_000);

    let result = h
        .escrow_client()
        .try_burn_from_on_settlement(&h.token_id, &h.investor, &101);
    assert_eq!(result, Err(Ok(Error::InsufficientAllowance)));
    assert_eq!(h.token.balance(&h.investor), 1_000);
}

#[test]
fn burn_from_on_settlement_rejects_a_missing_allowance() {
    let h = Harness::new(1_000);
    // No approve() call at all.
    let result = h
        .escrow_client()
        .try_burn_from_on_settlement(&h.token_id, &h.investor, &1);
    assert_eq!(result, Err(Ok(Error::InsufficientAllowance)));
}

#[test]
fn burn_from_on_settlement_rejects_an_expired_allowance() {
    let h = Harness::new(1_000);
    // expiration_ledger 0 with a nonzero amount is normally rejected by
    // approve() itself once the ledger has advanced past 0; set the ledger
    // forward first so the allowance is created already-expired... instead,
    // approve with a short expiration and then advance the ledger past it.
    h.token.approve(&h.investor, &h.escrow_id, &500, &5);
    h.env.ledger().with_mut(|l| l.sequence_number = 10);

    let result = h
        .escrow_client()
        .try_burn_from_on_settlement(&h.token_id, &h.investor, &100);
    assert_eq!(result, Err(Ok(Error::AllowanceExpired)));
}

#[test]
fn burn_on_settlement_rejects_when_paused() {
    let h = Harness::new(1_000);
    h.token.set_paused(&true);

    let result = h
        .escrow_client()
        .try_burn_on_settlement(&h.token_id, &h.investor, &100);
    assert_eq!(result, Err(Ok(Error::Paused)));
}

#[test]
fn burn_from_on_settlement_rejects_when_paused() {
    let h = Harness::new(1_000);
    h.token.approve(&h.investor, &h.escrow_id, &1_000, &1_000);
    h.token.set_paused(&true);

    let result = h
        .escrow_client()
        .try_burn_from_on_settlement(&h.token_id, &h.investor, &100);
    assert_eq!(result, Err(Ok(Error::Paused)));
}
