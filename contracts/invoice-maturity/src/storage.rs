//! Storage helpers for the invoice maturity contract.

use soroban_sdk::{Env, Symbol};

use crate::types::{Config, MaturityRecord, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn bump(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the global config from instance storage.
pub fn get_config(env: &Env) -> Option<Config> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Config)
}

/// Save the global config to instance storage.
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the maturity record for an invoice, extending its TTL on access.
pub fn get_maturity(env: &Env, invoice_id: &Symbol) -> Option<MaturityRecord> {
    let key = StorageKey::Maturity(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump(env, &key);
    }
    data
}

/// Save the maturity record for an invoice, extending its TTL.
pub fn set_maturity(env: &Env, invoice_id: &Symbol, data: &MaturityRecord) {
    let key = StorageKey::Maturity(invoice_id.clone());
    env.storage().persistent().set(&key, data);
    bump(env, &key);
}
