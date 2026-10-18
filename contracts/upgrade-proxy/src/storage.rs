//! Storage helpers for the upgrade proxy contract.

use soroban_sdk::{Address, Env};

use crate::types::{ProxyConfig, StorageKey, UpgradeProposal};

/// Ledgers below which an instance entry's TTL is extended.
pub const TTL_THRESHOLD: u32 = 120_960;
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the global config.
pub fn get_config(env: &Env) -> Option<ProxyConfig> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Config)
}

/// Save the global config.
pub fn set_config(env: &Env, config: &ProxyConfig) {
    env.storage().instance().set(&StorageKey::Config, config);
    extend_instance(env);
}

/// Get the current logic address.
pub fn get_logic(env: &Env) -> Option<Address> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Logic)
}

/// Set the current logic address.
pub fn set_logic(env: &Env, logic: &Address) {
    env.storage().instance().set(&StorageKey::Logic, logic);
    extend_instance(env);
}

/// Load the pending upgrade proposal, if any.
pub fn get_proposal(env: &Env) -> Option<UpgradeProposal> {
    extend_instance(env);
    env.storage().instance().get(&StorageKey::Proposal)
}

/// Save the pending upgrade proposal.
pub fn set_proposal(env: &Env, proposal: &UpgradeProposal) {
    env.storage()
        .instance()
        .set(&StorageKey::Proposal, proposal);
    extend_instance(env);
}

/// Clear the pending upgrade proposal.
pub fn clear_proposal(env: &Env) {
    env.storage().instance().remove(&StorageKey::Proposal);
    extend_instance(env);
}
