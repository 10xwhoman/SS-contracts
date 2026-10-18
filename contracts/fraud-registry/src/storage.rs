//! Storage helpers for the fraud report registry contract.

use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::types::{Config, FraudReport, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

fn bump_persistent(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
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
    env.storage()
        .instance()
        .get(&StorageKey::Initialized)
        .unwrap_or(false)
}

/// Mark the contract as initialized.
pub fn set_initialized(env: &Env) {
    env.storage()
        .instance()
        .set(&StorageKey::Initialized, &true);
    extend_instance(env);
}

/// Whether `investor` is whitelisted to submit reports.
pub fn is_whitelisted(env: &Env, investor: &Address) -> bool {
    let key = StorageKey::Whitelist(investor.clone());
    let whitelisted = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    whitelisted
}

/// Set (or clear) an investor's whitelist status.
pub fn set_whitelisted(env: &Env, investor: &Address, allowed: bool) {
    let key = StorageKey::Whitelist(investor.clone());
    if allowed {
        env.storage().persistent().set(&key, &true);
        bump_persistent(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// Load a report by id.
pub fn get_report(env: &Env, report_id: u32) -> Option<FraudReport> {
    let key = StorageKey::Report(report_id);
    let report = env.storage().persistent().get(&key);
    if report.is_some() {
        bump_persistent(env, &key);
    }
    report
}

/// Save a report by id.
pub fn set_report(env: &Env, report: &FraudReport) {
    let key = StorageKey::Report(report.report_id);
    env.storage().persistent().set(&key, report);
    bump_persistent(env, &key);
}

/// Get the list of report ids filed against an invoice.
pub fn get_reports_by_invoice(env: &Env, invoice: &BytesN<32>) -> Vec<u32> {
    let key = StorageKey::ReportsByInv(invoice.clone());
    let ids = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    ids
}

/// Append a report id to an invoice's report list.
pub fn add_report_to_invoice(env: &Env, invoice: &BytesN<32>, report_id: u32) {
    let key = StorageKey::ReportsByInv(invoice.clone());
    let mut ids: Vec<u32> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    ids.push_back(report_id);
    env.storage().persistent().set(&key, &ids);
    bump_persistent(env, &key);
}

/// Get the active report id for (invoice, reporter), if one exists.
pub fn get_active_report(env: &Env, invoice: &BytesN<32>, reporter: &Address) -> Option<u32> {
    let key = StorageKey::ActiveReport(invoice.clone(), reporter.clone());
    let id = env.storage().persistent().get(&key);
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
    }
    id
}

/// Set the active report id for (invoice, reporter).
pub fn set_active_report(env: &Env, invoice: &BytesN<32>, reporter: &Address, report_id: u32) {
    let key = StorageKey::ActiveReport(invoice.clone(), reporter.clone());
    env.storage().persistent().set(&key, &report_id);
    bump_persistent(env, &key);
}

/// Clear the active report id for (invoice, reporter).
pub fn clear_active_report(env: &Env, invoice: &BytesN<32>, reporter: &Address) {
    let key = StorageKey::ActiveReport(invoice.clone(), reporter.clone());
    env.storage().persistent().remove(&key);
}

/// Get and increment the report id counter, returning the next id.
pub fn next_report_id(env: &Env) -> u32 {
    extend_instance(env);
    let count: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::ReportCount)
        .unwrap_or(0);
    let next = count + 1;
    env.storage()
        .instance()
        .set(&StorageKey::ReportCount, &next);
    extend_instance(env);
    next
}

/// Load the invoice registry contract, if one is configured.
pub fn get_invoice_registry(env: &Env) -> Option<Address> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::InvoiceRegistry)
}

/// Save the invoice registry contract.
pub fn set_invoice_registry(env: &Env, registry: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::InvoiceRegistry, registry);
    extend_instance(env);
}
