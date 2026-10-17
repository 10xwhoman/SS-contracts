//! Storage helpers for the multi-sig admin contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::{Config, Proposal, StorageKey};

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

/// Load a proposal by id.
pub fn get_proposal(env: &Env, proposal_id: u32) -> Option<Proposal> {
    let key = StorageKey::Proposal(proposal_id);
    let proposal = env.storage().persistent().get(&key);
    if proposal.is_some() {
        bump(env, &key);
    }
    proposal
}

/// Save a proposal.
pub fn set_proposal(env: &Env, proposal_id: u32, proposal: &Proposal) {
    let key = StorageKey::Proposal(proposal_id);
    env.storage().persistent().set(&key, proposal);
    bump(env, &key);
}

/// Load the next proposal id (0 before the first proposal).
pub fn get_next_proposal_id(env: &Env) -> u32 {
    let key = StorageKey::NextProposalId;
    let id = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    id
}

/// Save the next proposal id.
pub fn set_next_proposal_id(env: &Env, id: u32) {
    let key = StorageKey::NextProposalId;
    env.storage().persistent().set(&key, &id);
    bump(env, &key);
}

/// Whether an account is whitelisted.
pub fn is_whitelisted(env: &Env, account: &Address) -> bool {
    let key = StorageKey::Whitelist(account.clone());
    let flag = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    flag
}

/// Set or clear an account's whitelist flag.
pub fn set_whitelisted(env: &Env, account: &Address, allowed: bool) {
    let key = StorageKey::Whitelist(account.clone());
    if allowed {
        env.storage().persistent().set(&key, &true);
        bump(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// Whether settlement has been triggered through the multi-sig for an invoice.
pub fn is_settlement_triggered(env: &Env, invoice_id: &Symbol) -> bool {
    let key = StorageKey::SettlementTriggered(invoice_id.clone());
    let flag = env.storage().persistent().get(&key).unwrap_or(false);
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    flag
}

/// Record a settlement trigger for an invoice.
pub fn set_settlement_triggered(env: &Env, invoice_id: &Symbol) {
    let key = StorageKey::SettlementTriggered(invoice_id.clone());
    env.storage().persistent().set(&key, &true);
    bump(env, &key);
}
