#![allow(deprecated, clippy::all)]

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use super::*;

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

fn setup() -> (Env, ReputationClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let contract_id = env.register(Reputation, ());
    let client = ReputationClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client = unsafe { core::mem::transmute::<_, ReputationClient<'static>>(client) };
    (env, client, admin, issuer)
}

// ── Initialization tests ─────────────────────────────────────────────────────

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Reputation, ());
    let client = ReputationClient::new(&env, &contract_id);
    client.initialize(&admin);

    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(crate::errors::Error::AlreadyInit)));
}

// ── record_settlement tests ────────────────────────────────────────────────

#[test]
fn test_record_settlement_admin_only() {
    let (env, client, _admin, issuer) = setup();

    // Non-admin should fail auth (no mock after set_auths).
    env.set_auths(&[]);
    let result = client.try_record_settlement(&issuer, &Symbol::new(&env, "INV1"), &0);
    assert!(result.is_err());
}

#[test]
fn test_record_settlement_stores_correctly() {
    let (env, client, _admin, issuer) = setup();

    client.record_settlement(&issuer, &Symbol::new(&env, "INV1"), &3); // 3 days early
    client.record_settlement(&issuer, &Symbol::new(&env, "INV2"), &-2); // 2 days late
    client.record_settlement(&issuer, &Symbol::new(&env, "INV3"), &0); // on time

    let history = client.get_settlement_history(&issuer);
    assert_eq!(history.len(), 3);
    assert_eq!(
        history.get(0).unwrap().invoice_id,
        Symbol::new(&env, "INV1")
    );
    assert_eq!(history.get(0).unwrap().days_offset, 3);
    assert_eq!(
        history.get(1).unwrap().invoice_id,
        Symbol::new(&env, "INV2")
    );
    assert_eq!(history.get(1).unwrap().days_offset, -2);
    assert_eq!(
        history.get(2).unwrap().invoice_id,
        Symbol::new(&env, "INV3")
    );
    assert_eq!(history.get(2).unwrap().days_offset, 0);
}

#[test]
fn test_reputation_updated_event_emitted() {
    let (env, client, _admin, issuer) = setup();

    client.record_settlement(&issuer, &Symbol::new(&env, "INV1"), &0);

    let events = env.events().all();
    let evt = events
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_, topics, _) = parse_event(&env, e);
            Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap()
                == Symbol::new(&env, "reputation_updated")
        })
        .expect("expected reputation_updated event");
    let (_, _, data) = parse_event(&env, evt);
    let (addr, score, _on_time, _var, count): (Address, u32, u32, i32, u32) =
        data.try_into_val(&env).unwrap();
    assert_eq!(addr, issuer);
    assert_eq!(score, 100); // single on-time settlement
    assert_eq!(count, 1);
}

// ── Score computation tests ────────────────────────────────────────────────

#[test]
fn test_score_empty_history() {
    let (_env, client, _admin, issuer) = setup();

    let score = client.get_score(&issuer);
    assert_eq!(score.score, 0);
    assert_eq!(score.components.settlement_count, 0);
    assert_eq!(score.components.on_time_rate, 0);
}

#[test]
fn test_score_all_on_time() {
    let (env, client, _admin, issuer) = setup();

    // Three on-time settlements (days_offset >= 0).
    client.record_settlement(&issuer, &Symbol::new(&env, "I1"), &0);
    client.record_settlement(&issuer, &Symbol::new(&env, "I2"), &1);
    client.record_settlement(&issuer, &Symbol::new(&env, "I3"), &2);

    let score = client.get_score(&issuer);
    assert_eq!(score.score, 100);
    assert_eq!(score.components.on_time_rate, 100);
    assert_eq!(score.components.avg_days_variance, 0); // early is never penalized
    assert_eq!(score.components.settlement_count, 3);
}

#[test]
fn test_score_all_late() {
    let (env, client, _admin, issuer) = setup();

    // Three late settlements.
    client.record_settlement(&issuer, &Symbol::new(&env, "I1"), &-1);
    client.record_settlement(&issuer, &Symbol::new(&env, "I2"), &-2);
    client.record_settlement(&issuer, &Symbol::new(&env, "I3"), &-3);

    let score = client.get_score(&issuer);
    // On-time rate = 0%, avg variance = (1+2+3)/3 = 2
    // on_time_bps = 0, variance_bps = (100 - 2*2) * 100 = 9600
    // total_bps = 0*60/100 + 9600*40/100 = 0 + 3840 = 3840
    // score = 3840 / 100 = 38
    assert_eq!(score.components.on_time_rate, 0);
    assert_eq!(score.components.avg_days_variance, 2);
    assert_eq!(score.score, 38);
    assert_eq!(score.components.settlement_count, 3);
}

#[test]
fn test_score_mixed_on_time_and_late() {
    let (env, client, _admin, issuer) = setup();

    // 4 on-time (days >= 0), 1 late (days < 0)
    client.record_settlement(&issuer, &Symbol::new(&env, "I1"), &0); // on time
    client.record_settlement(&issuer, &Symbol::new(&env, "I2"), &2); // early
    client.record_settlement(&issuer, &Symbol::new(&env, "I3"), &0); // on time
    client.record_settlement(&issuer, &Symbol::new(&env, "I4"), &1); // early
    client.record_settlement(&issuer, &Symbol::new(&env, "I5"), &-5); // late

    let score = client.get_score(&issuer);
    // On-time: 4/5 = 80%
    // avg days late = 5/5 = 1
    // on_time_bps = 8000, variance_bps = (100 - 1*2) * 100 = 9800
    // total_bps = 8000*60/100 + 9800*40/100 = 4800 + 3920 = 8720
    // score = 87
    assert_eq!(score.components.on_time_rate, 80);
    assert_eq!(score.components.avg_days_variance, 1);
    assert_eq!(score.score, 87);
}

#[test]
fn test_score_high_variance_caps_at_zero() {
    let (env, client, _admin, issuer) = setup();

    // A settlement 100 days late → variance penalty = 100*2 = 200 >> 100 → variance_score = 0
    client.record_settlement(&issuer, &Symbol::new(&env, "I1"), &-100);

    let score = client.get_score(&issuer);
    // on_time = 0%, variance = 100 → score should be 0
    assert_eq!(score.components.on_time_rate, 0);
    assert_eq!(score.components.avg_days_variance, 100);
    assert_eq!(score.score, 0);
}

// ── get_settlement_history tests ──────────────────────────────────────────

#[test]
fn test_get_settlement_history_returns_all_in_order() {
    let (env, client, _admin, issuer) = setup();

    let inv_ids = [
        Symbol::new(&env, "I0"),
        Symbol::new(&env, "I1"),
        Symbol::new(&env, "I2"),
        Symbol::new(&env, "I3"),
        Symbol::new(&env, "I4"),
    ];
    let offsets: [i32; 5] = [-2, -1, 0, 1, 2];
    for i in 0..5 {
        client.record_settlement(&issuer, &inv_ids[i], &offsets[i]);
    }

    let history = client.get_settlement_history(&issuer);
    assert_eq!(history.len(), 5);
    for i in 0..5 {
        let record = history.get(i as u32).unwrap();
        assert_eq!(record.invoice_id, inv_ids[i]);
        assert_eq!(record.days_offset, offsets[i]);
    }
}

#[test]
fn test_get_settlement_history_empty() {
    let (_env, client, _admin, issuer) = setup();

    let history = client.get_settlement_history(&issuer);
    assert_eq!(history.len(), 0);
}

// ── Multiple issuers are independent ────────────────────────────────────────

#[test]
fn test_multiple_issuers_independent() {
    let (env, client, _admin, _issuer) = setup();

    let issuer_a = Address::generate(&env);
    let issuer_b = Address::generate(&env);

    client.record_settlement(&issuer_a, &Symbol::new(&env, "A1"), &0);
    client.record_settlement(&issuer_b, &Symbol::new(&env, "B1"), &-10);

    let score_a = client.get_score(&issuer_a);
    let score_b = client.get_score(&issuer_b);

    assert_eq!(score_a.score, 100); // 1 on-time
                                    // 1 settlement late by 10 days: on-time rate 0%, avg days late 10,
                                    // variance score = 100 - 10*2 = 80 → total = 0 + 80*40/100 = 32
    assert_eq!(score_b.score, 32);

    let hist_a = client.get_settlement_history(&issuer_a);
    let hist_b = client.get_settlement_history(&issuer_b);
    assert_eq!(hist_a.len(), 1);
    assert_eq!(hist_b.len(), 1);
}

// ── Not-init errors ────────────────────────────────────────────────────────

#[test]
fn test_get_score_not_init() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Reputation, ());
    let client = ReputationClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);
    let result = client.try_get_score(&issuer);
    assert_eq!(result, Err(Ok(crate::errors::Error::NotInit)));
}

#[test]
fn test_record_settlement_not_init() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Reputation, ());
    let client = ReputationClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);
    let result = client.try_record_settlement(&issuer, &Symbol::new(&env, "INV1"), &0);
    assert_eq!(result, Err(Ok(crate::errors::Error::NotInit)));
}
