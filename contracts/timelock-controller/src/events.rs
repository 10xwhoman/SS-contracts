#![allow(deprecated)]
//! Event definitions for the timelock controller contract.

use soroban_sdk::{Address, Env, Symbol};

/// Emit an `ActionQueued` event when an action is added to the queue.
pub fn action_queued(
    env: &Env,
    action_id: u32,
    act_type: &Symbol,
    eta: u64,
    proposer: &Address,
) {
    env.events().publish(
        (
            Symbol::new(env, "ActionQueued"),
            action_id,
        ),
        (act_type.clone(), eta, proposer.clone()),
    );
}

/// Emit an `ActionExecuted` event when an action is executed after eta.
pub fn action_executed(env: &Env, action_id: u32, act_type: &Symbol, eta: u64) {
    env.events().publish(
        (
            Symbol::new(env, "ActionExecuted"),
            action_id,
        ),
        (act_type.clone(), eta),
    );
}

/// Emit an `ActionCancelled` event when an admin cancels a queued action.
pub fn action_cancelled(env: &Env, action_id: u32, act_type: &Symbol) {
    env.events().publish(
        (
            Symbol::new(env, "ActionCancelled"),
            action_id,
        ),
        act_type.clone(),
    );
}
