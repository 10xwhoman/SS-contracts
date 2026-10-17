//! Storage helpers for the timelock controller contract.

use soroban_sdk::Env;

use crate::types::{StorageKey, TimelockAction, TimelockConfig};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

fn bump_persistent(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the global config from instance storage.
pub fn get_config(env: &Env) -> Option<TimelockConfig> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Config)
}

/// Save the global config to instance storage.
pub fn set_config(env: &Env, config: &TimelockConfig) {
    env.storage().instance().set(&StorageKey::Config, config);
    extend_instance(env);
}

/// Load an action by id.
pub fn get_action(env: &Env, action_id: u32) -> Option<TimelockAction> {
    let key = StorageKey::Action(action_id);
    let action = env.storage().persistent().get(&key);
    if action.is_some() {
        bump_persistent(env, &key);
    }
    action
}

/// Save an action by id.
pub fn set_action(env: &Env, action: &TimelockAction) {
    let key = StorageKey::Action(action.action_id);
    env.storage().persistent().set(&key, action);
    bump_persistent(env, &key);
}

/// Get and increment the action id counter, returning the next id.
pub fn next_action_id(env: &Env) -> u32 {
    extend_instance(env);
    let count: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::Count)
        .unwrap_or(0);
    let next = count + 1;
    env.storage().instance().set(&StorageKey::Count, &next);
    extend_instance(env);
    next
}

/// Get the current action count (highest issued id).
pub fn get_action_count(env: &Env) -> u32 {
    extend_instance(env);
    env.storage()
        .instance()
        .get(&StorageKey::Count)
        .unwrap_or(0)
}
