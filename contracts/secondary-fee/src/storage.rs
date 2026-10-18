//! Storage helpers for the secondary market fee contract.

use soroban_sdk::{Address, Env};

use crate::types::{Sector, StorageKey};

/// Ledgers below which the instance entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    bump_instance(env);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::Treasury, treasury);
    bump_instance(env);
}

pub fn get_treasury(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&StorageKey::Treasury)
}

pub fn set_fee_bps(env: &Env, fee_bps: u32) {
    env.storage().instance().set(&StorageKey::FeeBps, &fee_bps);
    bump_instance(env);
}

pub fn get_fee_bps(env: &Env) -> u32 {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&StorageKey::FeeBps)
        .unwrap_or(0)
}

pub fn set_secondary_market(env: &Env, market: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::SecondaryMarket, market);
    bump_instance(env);
}

pub fn get_secondary_market(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&StorageKey::SecondaryMarket)
}

pub fn set_invoice_registry(env: &Env, registry: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::InvoiceRegistry, registry);
    bump_instance(env);
}

pub fn get_invoice_registry(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&StorageKey::InvoiceRegistry)
}

pub fn set_listing_fee(env: &Env, sector: Sector, fee_per_fraction: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::ListingFee(sector), &fee_per_fraction);
    bump_instance(env);
}

pub fn get_listing_fee(env: &Env, sector: Sector) -> i128 {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&StorageKey::ListingFee(sector))
        .unwrap_or(0)
}
