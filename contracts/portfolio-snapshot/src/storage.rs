//! Storage helpers for the portfolio snapshot contract.

use soroban_sdk::{Address, Env, Vec};

use crate::types::{SnapshotData, SnapshotInfo, StorageKey, TokenEntry};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

/// Extend the TTL of a persistent storage entry.
fn bump_persistent(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Admin)
}

/// Load the registered invoice-token contracts.
pub fn get_tokens(env: &Env) -> Vec<TokenEntry> {
    let key = StorageKey::Tokens;
    let data = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    data
}

/// Save the registered invoice-token contracts.
pub fn set_tokens(env: &Env, tokens: &Vec<TokenEntry>) {
    let key = StorageKey::Tokens;
    env.storage().persistent().set(&key, tokens);
    bump_persistent(env, &key);
}

/// Allocate and return the next snapshot id for a wallet (1-based).
pub fn next_snapshot_id(env: &Env, wallet: &Address) -> u64 {
    let key = StorageKey::NextSnapshotId(wallet.clone());
    let next: u64 = env.storage().persistent().get(&key).unwrap_or(1);
    env.storage().persistent().set(&key, &(next + 1));
    bump_persistent(env, &key);
    next
}

/// Load the ordered snapshot index for a wallet.
pub fn get_snapshot_list(env: &Env, wallet: &Address) -> Vec<SnapshotInfo> {
    let key = StorageKey::SnapshotList(wallet.clone());
    let data = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    data
}

/// Save the ordered snapshot index for a wallet.
pub fn set_snapshot_list(env: &Env, wallet: &Address, list: &Vec<SnapshotInfo>) {
    let key = StorageKey::SnapshotList(wallet.clone());
    if list.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, list);
        bump_persistent(env, &key);
    }
}

/// Load a snapshot payload for (wallet, snapshot_id).
pub fn get_snapshot(env: &Env, wallet: &Address, snapshot_id: u64) -> Option<SnapshotData> {
    let key = StorageKey::Snapshot(wallet.clone(), snapshot_id);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Save a snapshot payload for (wallet, snapshot_id).
pub fn set_snapshot(env: &Env, wallet: &Address, snapshot_id: u64, data: &SnapshotData) {
    let key = StorageKey::Snapshot(wallet.clone(), snapshot_id);
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Remove a snapshot payload for (wallet, snapshot_id).
pub fn remove_snapshot(env: &Env, wallet: &Address, snapshot_id: u64) {
    env.storage()
        .persistent()
        .remove(&StorageKey::Snapshot(wallet.clone(), snapshot_id));
}
