use soroban_sdk::{Env, Symbol, Vec};

/// Emitted when admin proposes a rebalance.
pub fn pool_rebalance_proposed(
    env: &Env,
    proposal_id: u32,
    pool_id: u32,
    new_weights: &Vec<u32>,
    executable_after: u64,
) {
    let topics = (Symbol::new(env, "PoolRebalanceProposed"), proposal_id);
    env.events()
        .publish(topics, (pool_id, new_weights.clone(), executable_after));
}

/// Emitted when a rebalance proposal is executed.
pub fn pool_rebalance_executed(
    env: &Env,
    proposal_id: u32,
    pool_id: u32,
    new_weights: &Vec<u32>,
) {
    let topics = (Symbol::new(env, "PoolRebalanceExecuted"), proposal_id);
    env.events()
        .publish(topics, (pool_id, new_weights.clone()));
}
