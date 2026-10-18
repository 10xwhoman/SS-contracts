#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::types::{FeeHistoryEntry, FeeSchedule, FeeType};

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

/// Helper to find a `FeeUpdated` event in the recorded events.
fn find_fee_updated_event(env: &Env) -> Option<(Address, Vec<Val>, Val)> {
    let all = env.events().all();
    let target = Symbol::new(env, "FeeUpdated");
    for event in all.events().iter() {
        let (_, topics, data) = parse_event(env, event);
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(env, &first) {
                if sym == target {
                    return Some((Address::generate(env), topics, data));
                }
            }
        }
    }
    None
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &500);
    let bounds = client.get_bounds();
    assert_eq!(bounds, (0, 500));
}

#[test]
fn test_initialize_rejects_double_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &500);
    let result = client.try_initialize(&admin, &0, &500);
    assert!(result.is_err());
}

#[test]
fn test_initialize_rejects_bad_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    let r = client.try_initialize(&admin, &500, &100);
    assert!(r.is_err());

    let r = client.try_initialize(&admin, &0, &10_001);
    assert!(r.is_err());
}

#[test]
fn test_set_fee_and_get_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);

    client.set_fee(&admin, &FeeType::Investment, &300);
    assert_eq!(client.get_fee(&FeeType::Investment), 300);

    client.set_fee(&admin, &FeeType::Secondary, &150);
    assert_eq!(client.get_fee(&FeeType::Secondary), 150);

    client.set_fee(&admin, &FeeType::Settlement, &50);
    assert_eq!(client.get_fee(&FeeType::Settlement), 50);
}

#[test]
fn test_set_fee_validates_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &100, &500);

    let r = client.try_set_fee(&admin, &FeeType::Investment, &50);
    assert!(r.is_err());

    let r = client.try_set_fee(&admin, &FeeType::Investment, &600);
    assert!(r.is_err());

    client.set_fee(&admin, &FeeType::Investment, &200);
    assert_eq!(client.get_fee(&FeeType::Investment), 200);
}

#[test]
fn test_set_fee_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let not_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);

    let r = client.try_set_fee(&not_admin, &FeeType::Investment, &100);
    assert!(r.is_err());
}

#[test]
fn test_get_all_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_fee(&admin, &FeeType::Investment, &300);
    client.set_fee(&admin, &FeeType::Secondary, &150);
    client.set_fee(&admin, &FeeType::Settlement, &75);

    let schedule: FeeSchedule = client.get_all_fees();
    assert_eq!(schedule.investment, 300);
    assert_eq!(schedule.secondary, 150);
    assert_eq!(schedule.settlement, 75);
}

#[test]
fn test_fee_updated_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_fee(&admin, &FeeType::Investment, &300);

    let event = find_fee_updated_event(&env);
    assert!(event.is_some(), "FeeUpdated event should be emitted");

    let (_, topics, data) = event.unwrap();
    let fee_type_val = topics.get(1).unwrap();
    let fee_type: u32 = fee_type_val.try_into_val(&env).unwrap();
    assert_eq!(fee_type, FeeType::Investment as u32);

    let tuple: (u32, u32, Address, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(tuple.0, 0);
    assert_eq!(tuple.1, 300);
    assert_eq!(tuple.2, admin);
}

#[test]
fn test_fee_updated_event_old_and_new() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_fee(&admin, &FeeType::Investment, &300);
    client.set_fee(&admin, &FeeType::Investment, &500);

    let event = find_fee_updated_event(&env);
    assert!(event.is_some());
    let (_, _topics, data) = event.unwrap();
    let tuple: (u32, u32, Address, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(tuple.0, 300);
    assert_eq!(tuple.1, 500);
}

#[test]
fn test_fee_history_descending() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_fee(&admin, &FeeType::Investment, &100);
    client.set_fee(&admin, &FeeType::Investment, &200);
    client.set_fee(&admin, &FeeType::Investment, &300);

    let history: Vec<FeeHistoryEntry> = client.get_fee_history(&FeeType::Investment);
    assert_eq!(history.len(), 3);

    let newest = history.get(0).unwrap();
    assert_eq!(newest.new_bps, 300);
    assert_eq!(newest.old_bps, 200);

    let middle = history.get(1).unwrap();
    assert_eq!(middle.new_bps, 200);
    assert_eq!(middle.old_bps, 100);

    let oldest = history.get(2).unwrap();
    assert_eq!(oldest.new_bps, 100);
    assert_eq!(oldest.old_bps, 0);
}

#[test]
fn test_fee_history_capped_at_50() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);

    for i in 0..60u32 {
        client.set_fee(&admin, &FeeType::Investment, &(i + 1));
    }

    let history: Vec<FeeHistoryEntry> = client.get_fee_history(&FeeType::Investment);
    assert_eq!(history.len(), 50, "history should be capped at 50 entries");

    let newest = history.get(0).unwrap();
    assert_eq!(newest.new_bps, 60);
    assert_eq!(newest.old_bps, 59);

    let oldest = history.get(49).unwrap();
    assert_eq!(oldest.new_bps, 11);
    assert_eq!(oldest.old_bps, 10);
}

#[test]
fn test_fee_history_isolated_per_type() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_fee(&admin, &FeeType::Investment, &100);
    client.set_fee(&admin, &FeeType::Secondary, &200);

    let inv_history: Vec<FeeHistoryEntry> = client.get_fee_history(&FeeType::Investment);
    let sec_history: Vec<FeeHistoryEntry> = client.get_fee_history(&FeeType::Secondary);

    assert_eq!(inv_history.len(), 1);
    assert_eq!(sec_history.len(), 1);
    assert_eq!(inv_history.get(0).unwrap().new_bps, 100);
    assert_eq!(sec_history.get(0).unwrap().new_bps, 200);
}

#[test]
fn test_set_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    client.initialize(&admin, &0, &1000);
    client.set_bounds(&admin, &200, &800);
    let bounds = client.get_bounds();
    assert_eq!(bounds, (200, 800));

    let r = client.try_set_fee(&admin, &FeeType::Investment, &100);
    assert!(r.is_err());

    client.set_fee(&admin, &FeeType::Investment, &500);
    assert_eq!(client.get_fee(&FeeType::Investment), 500);
}

#[test]
fn test_not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FeeConfig);
    let client = FeeConfigClient::new(&env, &contract_id);

    let r = client.try_get_fee(&FeeType::Investment);
    assert!(r.is_err());

    let r = client.try_get_all_fees();
    assert!(r.is_err());
}
