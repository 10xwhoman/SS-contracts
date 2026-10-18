//! Storage helpers for the reputation contract.

use soroban_sdk::{Address, Env, Vec};

use crate::types::{SettlementRecord, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a persistent entry's TTL to when bumped (~30 days at 5s/ledger).
const TTL_EXTEND_TO: u32 = 518_400;

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

/// Load the settlement history for an issuer, extending TTL on access.
pub fn get_settlements(env: &Env, issuer: &Address) -> Vec<SettlementRecord> {
    let key = StorageKey::Settlements(issuer.clone());
    let data: Option<Vec<SettlementRecord>> = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    data.unwrap_or_else(|| Vec::new(env))
}

/// Save the settlement history for an issuer, extending TTL.
pub fn set_settlements(env: &Env, issuer: &Address, records: &Vec<SettlementRecord>) {
    let key = StorageKey::Settlements(issuer.clone());
    env.storage().persistent().set(&key, records);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
