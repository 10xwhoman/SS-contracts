#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Symbol,
};

fn setup(env: &Env) -> (InvoiceCoInvestmentClient<'_>, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let contract_id = env.register(InvoiceCoInvestment, ());
    let client = InvoiceCoInvestmentClient::new(env, &contract_id);
    client.initialize(&admin, &token_id);
    (client, admin, token_id)
}

#[test]
fn create_syndicate_succeeds() {
    let env = Env::default();
    let (client, _admin, _token_id) = setup(&env);
    let lead = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(&invoice_id, &lead, &1_000_i128, &5_000_i128, &2_000u64);
    assert_eq!(sid, 1u32);
}

#[test]
fn join_syndicate_below_minimum_rejected() {
    let env = Env::default();
    let (client, _admin, token_id) = setup(&env);
    let lead = Address::generate(&env);
    let investor = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &5_000_i128,
        &2_000u64,
    );

    StellarAssetClient::new(&env, &token_id).mint(&investor, &10_000);
    assert_eq!(
        client.try_join_syndicate(&sid, &investor, &500_i128),
        Err(Ok(Error::BelowMinimum))
    );
}

#[test]
fn execute_syndicate_when_target_met() {
    let env = Env::default();
    let (client, _admin, token_id) = setup(&env);
    let lead = Address::generate(&env);
    let co1 = Address::generate(&env);
    let co2 = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &3_000_i128,
        &9_000u64,
    );

    StellarAssetClient::new(&env, &token_id).mint(&co1, &5_000);
    StellarAssetClient::new(&env, &token_id).mint(&co2, &5_000);

    client.join_syndicate(&sid, &co1, &2_000_i128);
    client.join_syndicate(&sid, &co2, &1_000_i128);

    // Target met – lead can execute
    client.execute_syndicate(&sid);
    let syndicate = client.get_syndicate(&sid);
    assert_eq!(syndicate.status, SyndicateStatus::Executed);
}

#[test]
fn execute_syndicate_before_target_rejected() {
    let env = Env::default();
    let (client, _admin, token_id) = setup(&env);
    let lead = Address::generate(&env);
    let co1 = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &5_000_i128,
        &9_000u64,
    );

    StellarAssetClient::new(&env, &token_id).mint(&co1, &5_000);
    client.join_syndicate(&sid, &co1, &2_000_i128);

    assert_eq!(
        client.try_execute_syndicate(&sid),
        Err(Ok(Error::TargetNotMet))
    );
}

#[test]
fn cancel_syndicate_returns_funds_after_deadline() {
    let env = Env::default();
    let (client, _admin, token_id) = setup(&env);
    let lead = Address::generate(&env);
    let co1 = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &10_000_i128,
        &2_000u64,
    );

    StellarAssetClient::new(&env, &token_id).mint(&co1, &5_000);
    client.join_syndicate(&sid, &co1, &1_500_i128);

    // Fast-forward past the deadline
    env.ledger().set_timestamp(3_000);
    client.cancel_syndicate(&sid);

    let syndicate = client.get_syndicate(&sid);
    assert_eq!(syndicate.status, SyndicateStatus::Cancelled);

    // co1 should have their funds back
    let bal = TokenClient::new(&env, &token_id).balance(&co1);
    assert_eq!(bal, 5_000);
}

#[test]
fn cancel_before_deadline_rejected() {
    let env = Env::default();
    let (client, _admin, _token_id) = setup(&env);
    let lead = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let sid = client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &5_000_i128,
        &9_000u64,
    );

    // Deadline not yet passed – should fail
    assert_eq!(
        client.try_cancel_syndicate(&sid),
        Err(Ok(Error::FundingWindowExpired))
    );
}

#[test]
fn syndicate_created_event_emitted() {
    let env = Env::default();
    let (client, _admin, _token) = setup(&env);
    let lead = Address::generate(&env);
    env.ledger().set_timestamp(1_000);
    client.create_syndicate(
        &Symbol::new(&env, "INV001"),
        &lead,
        &1_000_i128,
        &5_000_i128,
        &2_000u64,
    );
    assert!(env.events().all().events().len() >= 1);
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let (client, admin, token_id) = setup(&env);
    assert_eq!(
        client.try_initialize(&admin, &token_id),
        Err(Ok(Error::AlreadyInitialized))
    );
}
