//! Storage helpers for the referral contract.

use soroban_sdk::{Address, Env};

use crate::types::{Config, StorageKey};

/// Load contract config from instance storage.
pub fn get_config(env: &Env) -> Option<Config> {
    env.storage().instance().get(&StorageKey::Config)
}

/// Save contract config to instance storage.
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
}

/// Get the referrer for a given referee. Returns None if not registered.
pub fn get_referrer(env: &Env, referee: &Address) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::Referrer(referee.clone()))
}

/// Set the referrer for a given referee (one-time binding).
pub fn set_referrer(env: &Env, referee: &Address, referrer: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::Referrer(referee.clone()), referrer);
}

/// Get the pending (unclaimed) rewards for a referrer.
pub fn get_pending(env: &Env, referrer: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Pending(referrer.clone()))
        .unwrap_or(0)
}

/// Set the pending rewards for a referrer.
pub fn set_pending(env: &Env, referrer: &Address, amount: i128) {
    if amount == 0 {
        env.storage()
            .persistent()
            .remove(&StorageKey::Pending(referrer.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&StorageKey::Pending(referrer.clone()), &amount);
    }
}

/// Get the total lifetime rewards earned by a referrer.
pub fn get_total_earned(env: &Env, referrer: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::TotalEarned(referrer.clone()))
        .unwrap_or(0)
}

/// Set the total lifetime rewards earned by a referrer.
pub fn set_total_earned(env: &Env, referrer: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TotalEarned(referrer.clone()), &amount);
}

/// Get the number of referees registered under a referrer.
pub fn get_ref_count(env: &Env, referrer: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&StorageKey::RefCount(referrer.clone()))
        .unwrap_or(0)
}

/// Set the number of referees registered under a referrer.
pub fn set_ref_count(env: &Env, referrer: &Address, count: u32) {
    env.storage()
        .persistent()
        .set(&StorageKey::RefCount(referrer.clone()), &count);
}
