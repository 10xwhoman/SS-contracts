//! Storage helpers for the invoice registry contract.

use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::types::{CategoryAssignment, InvoiceCategory, StorageKey};

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

/// Load the classification assigned to an invoice.
pub fn get_assignment(env: &Env, invoice_id: &BytesN<32>) -> Option<CategoryAssignment> {
    let key = StorageKey::Assignment(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Save the classification assigned to an invoice.
pub fn set_assignment(env: &Env, invoice_id: &BytesN<32>, data: &CategoryAssignment) {
    let key = StorageKey::Assignment(invoice_id.clone());
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Remove the classification of an invoice.
pub fn remove_assignment(env: &Env, invoice_id: &BytesN<32>) {
    env.storage()
        .persistent()
        .remove(&StorageKey::Assignment(invoice_id.clone()));
}

/// Load the ordered invoice index for a category.
pub fn get_index(env: &Env, category: InvoiceCategory) -> Vec<BytesN<32>> {
    let key = StorageKey::CategoryIndex(category);
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

/// Save the ordered invoice index for a category.
pub fn set_index(env: &Env, category: InvoiceCategory, index: &Vec<BytesN<32>>) {
    let key = StorageKey::CategoryIndex(category);
    env.storage().persistent().set(&key, index);
    bump_persistent(env, &key);
}

/// Set the contract allowed to flag invoices as under review.
pub fn set_review_authority(env: &Env, authority: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::ReviewAuthority, authority);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the contract allowed to flag invoices as under review.
pub fn get_review_authority(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::ReviewAuthority)
}

/// Whether an invoice is currently flagged as under review.
pub fn is_under_review(env: &Env, invoice_id: &BytesN<32>) -> bool {
    let key = StorageKey::UnderReview(invoice_id.clone());
    let flagged = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    flagged
}

/// Set or clear an invoice's under-review flag.
pub fn set_under_review(env: &Env, invoice_id: &BytesN<32>, flagged: bool) {
    let key = StorageKey::UnderReview(invoice_id.clone());
    if flagged {
        env.storage().persistent().set(&key, &true);
        bump_persistent(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}
