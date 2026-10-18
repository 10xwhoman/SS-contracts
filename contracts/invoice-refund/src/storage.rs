//! Storage helpers for the invoice refund contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::{Config, RefundInvoice, StorageKey};

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

/// Load an invoice record, extending its TTL on access.
pub fn get_invoice(env: &Env, invoice_id: &Symbol) -> Option<RefundInvoice> {
    let key = StorageKey::Invoice(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump(env, &key);
    }
    data
}

/// Save an invoice record, extending its TTL.
pub fn set_invoice(env: &Env, invoice_id: &Symbol, data: &RefundInvoice) {
    let key = StorageKey::Invoice(invoice_id.clone());
    env.storage().persistent().set(&key, data);
    bump(env, &key);
}

/// Whether an invoice is registered.
pub fn has_invoice(env: &Env, invoice_id: &Symbol) -> bool {
    let key = StorageKey::Invoice(invoice_id.clone());
    let exists = env.storage().persistent().has(&key);
    if exists {
        bump(env, &key);
    }
    exists
}

/// Load an investor's locked amount for an invoice.
pub fn get_investment(env: &Env, invoice_id: &Symbol, investor: &Address) -> i128 {
    let key = StorageKey::Investment(invoice_id.clone(), investor.clone());
    let amount = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    amount
}

/// Save (or clear, when zero) an investor's locked amount for an invoice.
pub fn set_investment(env: &Env, invoice_id: &Symbol, investor: &Address, amount: i128) {
    let key = StorageKey::Investment(invoice_id.clone(), investor.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump(env, &key);
    }
}
