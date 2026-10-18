//! Storage helpers for the fee configuration contract.

use soroban_sdk::{Env, Vec};

use crate::types::{Config, FeeHistoryEntry, FeeType, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

/// Maximum number of historical entries kept per fee type.
pub const MAX_HISTORY: u32 = 50;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the global config from instance storage.
pub fn get_config(env: &Env) -> Option<Config> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Config)
}

/// Save the global config to instance storage.
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
    extend_instance(env);
}

/// Check whether the contract has been initialized.
pub fn is_initialized(env: &Env) -> bool {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Initialized).unwrap_or(false)
}

/// Mark the contract as initialized.
pub fn set_initialized(env: &Env) {
    env.storage().instance().set(&StorageKey::Initialized, &true);
    extend_instance(env);
}

/// Read the current rate (bps) for a fee type. Defaults to 0 when unset.
pub fn get_fee_rate(env: &Env, fee_type: FeeType) -> u32 {
    extend_instance(env);
    env.storage()
        .instance()
        .get(&StorageKey::FeeRate(fee_type))
        .unwrap_or(0)
}

/// Store the current rate (bps) for a fee type.
pub fn set_fee_rate(env: &Env, fee_type: FeeType, rate_bps: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::FeeRate(fee_type), &rate_bps);
    extend_instance(env);
}

/// Load the change history (newest first) for a fee type, if any.
pub fn get_fee_history(env: &Env, fee_type: FeeType) -> Vec<FeeHistoryEntry> {
    let key = StorageKey::FeeHistory(fee_type);
    let history = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    history
}

/// Save the change history for a fee type.
pub fn set_fee_history(env: &Env, fee_type: FeeType, history: &Vec<FeeHistoryEntry>) {
    let key = StorageKey::FeeHistory(fee_type);
    env.storage().persistent().set(&key, history);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
