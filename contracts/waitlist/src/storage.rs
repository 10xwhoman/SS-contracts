//! Storage helpers for the waitlist contract.

use soroban_sdk::{Address, BytesN, Env};

use crate::types::{Config, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a persistent entry's TTL to when bumped (~30 days at 5s/ledger).
const TTL_EXTEND_TO: u32 = 518_400;

fn extend_persistent_ttl(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_config(env: &Env) -> Option<Config> {
    env.storage().instance().get(&StorageKey::Config)
}

pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
}

pub fn get_next_slot(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::NextSlot)
        .unwrap_or(1)
}

pub fn set_next_slot(env: &Env, slot: u32) {
    env.storage().instance().set(&StorageKey::NextSlot, &slot);
}

pub fn get_wallet_at(env: &Env, slot: u32) -> Option<Address> {
    let key = StorageKey::QueueSlot(slot);
    let addr = env.storage().persistent().get(&key);
    if addr.is_some() {
        extend_persistent_ttl(env, &key);
    }
    addr
}

pub fn set_wallet_at(env: &Env, slot: u32, wallet: &Address) {
    let key = StorageKey::QueueSlot(slot);
    env.storage().persistent().set(&key, wallet);
    extend_persistent_ttl(env, &key);
}

pub fn get_wallet_pos(env: &Env, wallet: &Address) -> u32 {
    let key = StorageKey::WalletPos(wallet.clone());
    let pos: Option<u32> = env.storage().persistent().get(&key);
    if pos.is_some() {
        extend_persistent_ttl(env, &key);
    }
    pos.unwrap_or(0)
}

pub fn set_wallet_pos(env: &Env, wallet: &Address, pos: u32) {
    let key = StorageKey::WalletPos(wallet.clone());
    env.storage().persistent().set(&key, &pos);
    extend_persistent_ttl(env, &key);
}

/// Test-only accessor for the stored email hash. The hash is write-only from
/// the contract's perspective (readable by off-chain indexers via ledger
/// entries), so no public entrypoint exposes it.
#[cfg(test)]
pub fn get_wallet_email(env: &Env, wallet: &Address) -> Option<BytesN<32>> {
    let key = StorageKey::WalletEml(wallet.clone());
    let email = env.storage().persistent().get(&key);
    if email.is_some() {
        extend_persistent_ttl(env, &key);
    }
    email
}

pub fn set_wallet_email(env: &Env, wallet: &Address, email_hash: &BytesN<32>) {
    let key = StorageKey::WalletEml(wallet.clone());
    env.storage().persistent().set(&key, email_hash);
    extend_persistent_ttl(env, &key);
}

pub fn is_approved(env: &Env, wallet: &Address) -> bool {
    let key = StorageKey::Approved(wallet.clone());
    let approved: Option<bool> = env.storage().persistent().get(&key);
    if approved.is_some() {
        extend_persistent_ttl(env, &key);
    }
    approved.unwrap_or(false)
}

pub fn set_approved(env: &Env, wallet: &Address, approved: bool) {
    let key = StorageKey::Approved(wallet.clone());
    if approved {
        env.storage().persistent().set(&key, &true);
    } else {
        env.storage().persistent().remove(&key);
    }
    extend_persistent_ttl(env, &key);
}
