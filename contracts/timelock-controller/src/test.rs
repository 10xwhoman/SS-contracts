#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec};

use crate::types::{PendingAction, TimelockAction};

/// Parse an XDR ContractEvent into (contract_addr, topics, data).
fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(
                hash.clone(),
            )),
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

/// Check whether an event with the given name was emitted.
fn has_event(env: &Env, name: &str) -> bool {
    let all = env.events().all();
    let target = Symbol::new(env, name);
    for event in all.events().iter() {
        let (_, topics, _) = parse_event(env, event);
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(env, &first) {
                if sym == target {
                    return true;
                }
            }
        }
    }
    false
}

fn make_params(env: &Env) -> Vec<Val> {
    soroban_sdk::vec![env, 100i128.into_val(env), Symbol::new(env, "x").to_val()]
}

struct Setup {
    env: Env,
    client: TimelockControllerClient<'static>,
    admin: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, TimelockController);
        let client = TimelockControllerClient::new(&env, &contract_id);
        client.initialize(&admin, &0); // 0 → default 48h
        Self { env, client, admin }
    }
}

#[test]
fn test_initialize_default_delay() {
    let s = Setup::new();
    let delay = s.client.get_min_delay();
    assert_eq!(delay, 48 * 3_600);
}

#[test]
fn test_initialize_custom_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &3600);
    assert_eq!(client.get_min_delay(), 3600);
}

#[test]
fn test_initialize_rejects_double_init() {
    let s = Setup::new();
    let r = s.client.try_initialize(&s.admin, &0);
    assert!(r.is_err());
}

#[test]
fn test_queue_action_enforces_min_delay() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let params = make_params(&s.env);
    let act_type = Symbol::new(&s.env, "upgrade");

    // eta exactly at now + min_delay should succeed.
    let eta = now + 48 * 3_600;
    let id = s.client.queue_action(&s.admin, &act_type, &params, &eta);
    assert_eq!(id, 1);

    // eta below min_delay should fail.
    let too_soon = now + 100;
    let r = s
        .client
        .try_queue_action(&s.admin, &act_type, &params, &too_soon);
    assert!(r.is_err());
}

#[test]
fn test_queue_action_rejects_zero_eta() {
    let s = Setup::new();
    let params = make_params(&s.env);
    let act_type = Symbol::new(&s.env, "upgrade");
    let r = s
        .client
        .try_queue_action(&s.admin, &act_type, &params, &0);
    assert!(r.is_err());
}

#[test]
fn test_execute_blocked_before_eta() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let params = make_params(&s.env);
    let act_type = Symbol::new(&s.env, "upgrade");

    let eta = now + 48 * 3_600;
    let id = s
        .client
        .queue_action(&s.admin, &act_type, &params, &eta);

    // Try to execute immediately — should fail.
    let r = s.client.try_execute_action(&id);
    assert!(r.is_err());
}

#[test]
fn test_execute_after_eta_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);

    // Use a small delay for testing.
    client.initialize(&admin, &10); // 10-second min delay

    let now = env.ledger().timestamp();
    let params = make_params(&env);
    let act_type = Symbol::new(&env, "upgrade");
    let eta = now + 10;
    let id = client.queue_action(&admin, &act_type, &params, &eta);

    // Advance time past eta.
    env.ledger().set_timestamp(eta + 1);

    client.execute_action(&id);

    let action: TimelockAction = client.get_action(&id);
    assert!(action.executed);
    assert!(!action.cancelled);
}

#[test]
fn test_execute_nonexistent_action() {
    let s = Setup::new();
    let r = s.client.try_execute_action(&999);
    assert!(r.is_err());
}

#[test]
fn test_double_execute_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &10);

    let now = env.ledger().timestamp();
    let params = make_params(&env);
    let eta = now + 10;
    let id = client.queue_action(&admin, &Symbol::new(&env, "u"), &params, &eta);

    env.ledger().set_timestamp(eta + 1);
    client.execute_action(&id);

    let r = client.try_execute_action(&id);
    assert!(r.is_err());
}

#[test]
fn test_cancel_action_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &10);

    let now = env.ledger().timestamp();
    let params = make_params(&env);
    let eta = now + 10;
    let id = client.queue_action(&admin, &Symbol::new(&env, "u"), &params, &eta);

    // Admin cancels.
    client.cancel_action(&id);

    let action: TimelockAction = client.get_action(&id);
    assert!(action.cancelled);
    assert!(!action.executed);

    // Cancelled action cannot be executed.
    env.ledger().set_timestamp(eta + 1);
    let r = client.try_execute_action(&id);
    assert!(r.is_err());
}

#[test]
fn test_cancel_nonexistent_action() {
    let s = Setup::new();
    let r = s.client.try_cancel_action(&999);
    assert!(r.is_err());
}

#[test]
fn test_cancel_already_executed() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &10);

    let now = env.ledger().timestamp();
    let params = make_params(&env);
    let eta = now + 10;
    let id = client.queue_action(&admin, &Symbol::new(&env, "u"), &params, &eta);

    env.ledger().set_timestamp(eta + 1);
    client.execute_action(&id);

    let r = client.try_cancel_action(&id);
    assert!(r.is_err());
}

#[test]
fn test_get_queued_actions() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &10);

    let now = env.ledger().timestamp();
    let params = make_params(&env);

    let id1 = client.queue_action(&admin, &Symbol::new(&env, "a"), &params, &(now + 10));
    let id2 = client.queue_action(&admin, &Symbol::new(&env, "b"), &params, &(now + 20));
    let id3 = client.queue_action(&admin, &Symbol::new(&env, "c"), &params, &(now + 30));

    // Cancel one.
    client.cancel_action(&id2);

    // Execute one.
    env.ledger().set_timestamp(now + 11);
    client.execute_action(&id1);

    // Only id3 should be pending.
    let pending: Vec<PendingAction> = client.get_queued_actions();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get(0).unwrap().action_id, id3);
    assert_eq!(pending.get(0).unwrap().eta, now + 30);
}

#[test]
fn test_get_queued_actions_returns_params_and_eta() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let params = make_params(&s.env);
    let eta = now + 48 * 3_600;

    let id = s
        .client
        .queue_action(&s.admin, &Symbol::new(&s.env, "x"), &params, &eta);

    let pending: Vec<PendingAction> = s.client.get_queued_actions();
    assert_eq!(pending.len(), 1);
    let action = pending.get(0).unwrap();
    assert_eq!(action.action_id, id);
    assert_eq!(action.eta, eta);
    assert_eq!(action.act_type, Symbol::new(&s.env, "x"));
    assert_eq!(action.params.len(), 2);
}

#[test]
fn test_action_queued_event() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let params = make_params(&s.env);
    let eta = now + 48 * 3_600;
    s.client
        .queue_action(&s.admin, &Symbol::new(&s.env, "x"), &params, &eta);

    assert!(has_event(&s.env, "ActionQueued"));
}

#[test]
fn test_action_executed_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TimelockController);
    let client = TimelockControllerClient::new(&env, &contract_id);
    client.initialize(&admin, &10);

    let now = env.ledger().timestamp();
    let params = make_params(&env);
    let eta = now + 10;
    let id = client.queue_action(&admin, &Symbol::new(&env, "x"), &params, &eta);

    env.ledger().set_timestamp(eta + 1);
    client.execute_action(&id);

    assert!(has_event(&env, "ActionExecuted"));
}

#[test]
fn test_set_min_delay() {
    let s = Setup::new();
    s.client.set_min_delay(&100);
    assert_eq!(s.client.get_min_delay(), 100);

    // Now queueing requires eta >= now + 100.
    let now = s.env.ledger().timestamp();
    let params = make_params(&s.env);
    let r = s
        .client
        .try_queue_action(&s.admin, &Symbol::new(&s.env, "x"), &params, &(now + 50));
    assert!(r.is_err());

    let id = s
        .client
        .queue_action(&s.admin, &Symbol::new(&s.env, "x"), &params, &(now + 100));
    assert_eq!(id, 1);
}

#[test]
fn test_set_min_delay_rejects_zero() {
    let s = Setup::new();
    let r = s.client.try_set_min_delay(&0);
    assert!(r.is_err());
}
