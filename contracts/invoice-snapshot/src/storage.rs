//! Storage helpers for the investor position snapshot contract.

use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::{InvestorSnapshot, Snapshot, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn bump(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the admin from instance storage.
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Admin)
}

/// Save the admin to instance storage.
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the ordered holder list for an invoice.
pub fn get_holders(env: &Env, invoice_id: &Symbol) -> Vec<Address> {
    let key = StorageKey::Holders(invoice_id.clone());
    let holders: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    holders
}

/// Save the ordered holder list for an invoice.
pub fn set_holders(env: &Env, invoice_id: &Symbol, holders: &Vec<Address>) {
    let key = StorageKey::Holders(invoice_id.clone());
    if holders.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, holders);
        bump(env, &key);
    }
}

/// Load a holder's current balance for an invoice (0 when absent).
pub fn get_holder_balance(env: &Env, invoice_id: &Symbol, holder: &Address) -> i128 {
    let key = StorageKey::HolderBalance(invoice_id.clone(), holder.clone());
    let amount = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    amount
}

/// Save (or clear, when zero) a holder's balance for an invoice.
pub fn set_holder_balance(env: &Env, invoice_id: &Symbol, holder: &Address, amount: i128) {
    let key = StorageKey::HolderBalance(invoice_id.clone(), holder.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump(env, &key);
    }
}

/// Load the last snapshot id assigned for an invoice (0 when none).
pub fn get_last_snapshot_id(env: &Env, invoice_id: &Symbol) -> u32 {
    let key = StorageKey::NextSnapshotId(invoice_id.clone());
    let id = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    id
}

/// Save the last snapshot id assigned for an invoice.
pub fn set_last_snapshot_id(env: &Env, invoice_id: &Symbol, id: u32) {
    let key = StorageKey::NextSnapshotId(invoice_id.clone());
    env.storage().persistent().set(&key, &id);
    bump(env, &key);
}

/// Load a snapshot by (invoice_id, snapshot_id).
pub fn get_snapshot(env: &Env, invoice_id: &Symbol, snapshot_id: u32) -> Option<Snapshot> {
    let key = StorageKey::Snapshot(invoice_id.clone(), snapshot_id);
    let snapshot = env.storage().persistent().get(&key);
    if snapshot.is_some() {
        bump(env, &key);
    }
    snapshot
}

/// Save a snapshot.
pub fn set_snapshot(env: &Env, invoice_id: &Symbol, snapshot_id: u32, snapshot: &Snapshot) {
    let key = StorageKey::Snapshot(invoice_id.clone(), snapshot_id);
    env.storage().persistent().set(&key, snapshot);
    bump(env, &key);
}

/// Remove a snapshot.
pub fn remove_snapshot(env: &Env, invoice_id: &Symbol, snapshot_id: u32) {
    env.storage()
        .persistent()
        .remove(&StorageKey::Snapshot(invoice_id.clone(), snapshot_id));
}

/// Load the ordered live snapshot ids for an invoice.
pub fn get_snapshot_index(env: &Env, invoice_id: &Symbol) -> Vec<u32> {
    let key = StorageKey::SnapshotIndex(invoice_id.clone());
    let index: Vec<u32> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    index
}

/// Save the ordered live snapshot ids for an invoice.
pub fn set_snapshot_index(env: &Env, invoice_id: &Symbol, index: &Vec<u32>) {
    let key = StorageKey::SnapshotIndex(invoice_id.clone());
    if index.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, index);
        bump(env, &key);
    }
}

/// Load a wallet's cross-invoice snapshot references, oldest first.
pub fn get_wallet_index(env: &Env, wallet: &Address) -> Vec<InvestorSnapshot> {
    let key = StorageKey::WalletIndex(wallet.clone());
    let index: Vec<InvestorSnapshot> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    index
}

/// Save a wallet's cross-invoice snapshot references.
pub fn set_wallet_index(env: &Env, wallet: &Address, index: &Vec<InvestorSnapshot>) {
    let key = StorageKey::WalletIndex(wallet.clone());
    if index.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, index);
        bump(env, &key);
    }
}
