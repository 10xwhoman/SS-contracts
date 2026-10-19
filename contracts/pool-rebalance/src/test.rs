#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Vec,
};

fn setup(env: &Env) -> (PoolRebalanceClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(PoolRebalance, ());
    let client = PoolRebalanceClient::new(env, &contract_id);
    // Use a 10-second timelock for easier testing
    client.initialize(&admin, &10u64);
    (client, admin)
}

fn two_weights(env: &Env, a: u32, b: u32) -> Vec<u32> {
    let mut v = Vec::new(env);
    v.push_back(a);
    v.push_back(b);
    v
}

#[test]
fn register_and_propose_rebalance() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));

    env.ledger().set_timestamp(100);
    let proposal_id = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));
    assert_eq!(proposal_id, 1u32);
}

#[test]
fn weights_not_summing_to_10000_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));

    env.ledger().set_timestamp(100);
    assert_eq!(
        client.try_propose_rebalance(&1u32, &two_weights(&env, 5_000, 4_000)),
        Err(Ok(Error::InvalidWeights))
    );
}

#[test]
fn execute_before_timelock_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));

    env.ledger().set_timestamp(100);
    let pid = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));

    // Still within timelock window (only 5 seconds later)
    env.ledger().set_timestamp(105);
    assert_eq!(
        client.try_execute_rebalance(&pid),
        Err(Ok(Error::TimeNotReached))
    );
}

#[test]
fn execute_after_timelock_succeeds() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));

    env.ledger().set_timestamp(100);
    let pid = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));

    // Past 10-second timelock
    env.ledger().set_timestamp(120);
    client.execute_rebalance(&pid);

    let pool = client.get_pool(&1u32);
    assert_eq!(pool.weights.get(0).unwrap(), 6_000u32);
    assert_eq!(pool.weights.get(1).unwrap(), 4_000u32);
}

#[test]
fn investor_positions_recomputed_on_execute() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let investor = Address::generate(&env);

    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));
    // Record a position of 1000 with equal weights → allocations [500, 500]
    client.record_position(&1u32, &investor, &1_000_i128);

    let pos_before = client.get_position(&1u32, &investor);
    assert_eq!(pos_before.allocations.get(0).unwrap(), 500_i128);
    assert_eq!(pos_before.allocations.get(1).unwrap(), 500_i128);

    env.ledger().set_timestamp(100);
    let pid = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));
    env.ledger().set_timestamp(120);
    client.execute_rebalance(&pid);

    let pos_after = client.get_position(&1u32, &investor);
    // 1000 * 6000/10000 = 600, remainder = 400
    assert_eq!(pos_after.allocations.get(0).unwrap(), 600_i128);
    assert_eq!(pos_after.allocations.get(1).unwrap(), 400_i128);
}

#[test]
fn get_rebalance_preview_returns_accurate_impact() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let investor = Address::generate(&env);

    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));
    client.record_position(&1u32, &investor, &2_000_i128);

    let new_w = two_weights(&env, 7_000, 3_000);
    let impacts = client.get_rebalance_preview(&1u32, &new_w);
    assert_eq!(impacts.len(), 1);
    let impact = impacts.get(0).unwrap();
    // old: [1000, 1000]  new: [1400, 600]
    assert_eq!(impact.old_allocations.get(0).unwrap(), 1_000_i128);
    assert_eq!(impact.new_allocations.get(0).unwrap(), 1_400_i128);
    assert_eq!(impact.new_allocations.get(1).unwrap(), 600_i128);
}

#[test]
fn pool_rebalance_proposed_event_emitted() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));
    env.ledger().set_timestamp(100);
    client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));
    assert!(env.events().all().events().len() >= 1);
}

#[test]
fn pool_rebalance_executed_event_emitted() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));
    env.ledger().set_timestamp(100);
    let pid = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));
    env.ledger().set_timestamp(120);
    client.execute_rebalance(&pid);
    assert!(env.events().all().events().len() >= 1);
}

#[test]
fn double_execute_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.register_pool(&1u32, &two_weights(&env, 5_000, 5_000));
    env.ledger().set_timestamp(100);
    let pid = client.propose_rebalance(&1u32, &two_weights(&env, 6_000, 4_000));
    env.ledger().set_timestamp(120);
    client.execute_rebalance(&pid);
    assert_eq!(
        client.try_execute_rebalance(&pid),
        Err(Ok(Error::AlreadyExecuted))
    );
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_eq!(
        client.try_initialize(&admin, &10u64),
        Err(Ok(Error::AlreadyInitialized))
    );
}
