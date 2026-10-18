use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::{DistributionState, FeeTier, StorageKey};

/// Ledgers below which a `Distribution` persistent entry's TTL is extended
/// (~7 days at 5s/ledger). Issue #128.
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a `Distribution` persistent entry's TTL to when bumped
/// (~30 days at 5s/ledger). Issue #128.
const TTL_EXTEND_TO: u32 = 518_400;

/// Extend the TTL of a distribution's persistent storage entry so it survives
/// ledger pruning across the full lifetime of a long-lived invoice fee
/// schedule. Issue #128.
pub fn extend_ttl(env: &Env, escrow: &Address, invoice_id: &Symbol) {
    env.storage().persistent().extend_ttl(
        &StorageKey::Distribution(escrow.clone(), invoice_id.clone()),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn set_fee_recipient(env: &Env, fee_recipient: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::FeeRecipient, fee_recipient);
}

pub fn get_fee_recipient(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::FeeRecipient)
}

/// Whitelisted escrow contract address accessors (Issue #121).
pub fn set_escrow_contract(env: &Env, escrow_contract: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::EscrowContract, escrow_contract);
}

pub fn get_escrow_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::EscrowContract)
}

/// Re-entrancy guard flag accessors (Issue #127).
///
/// The guard is set on entry to a guarded entrypoint and cleared on exit, so it
/// only ever needs to be observable *within* one invocation. It therefore lives
/// in temporary storage rather than instance storage (issue #492):
///
///   * a re-entrant call necessarily happens in the same ledger, and temporary
///     entries are alive for the whole of it, so the guard is fully effective
///     for the case it exists to catch;
///   * temporary entries are much cheaper than instance entries and are not
///     part of the archived contract instance that every invocation has to
///     load, so a flag that is meaningless between calls should not be paying
///     rent there;
///   * it is self-healing. If any path ever left the flag set, an instance flag
///     would brick every guarded entrypoint until an admin manually intervened,
///     because nothing would clear it. A temporary entry expires on its own.
///
/// Soroban rolls back all storage writes when an invocation returns an error,
/// so a guard set on a path that then fails is cleared either way.
pub fn is_locked(env: &Env) -> bool {
    env.storage()
        .temporary()
        .get(&StorageKey::Locked)
        .unwrap_or(false)
}

pub fn set_lock(env: &Env, locked: bool) {
    env.storage().temporary().set(&StorageKey::Locked, &locked);
}

pub fn get_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
) -> Option<DistributionState> {
    let data = env.storage().persistent().get(&StorageKey::Distribution(
        escrow.clone(),
        invoice_id.clone(),
    ));
    if data.is_some() {
        extend_ttl(env, escrow, invoice_id);
    }
    data
}

pub fn set_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    state: &DistributionState,
) {
    env.storage().persistent().set(
        &StorageKey::Distribution(escrow.clone(), invoice_id.clone()),
        state,
    );
    extend_ttl(env, escrow, invoice_id);
}

// ==================== Role-based access control (Issue #182) ====================

#[allow(dead_code)]
pub fn get_role_admin(env: &Env, role: &Symbol) -> Option<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::RoleAdmin(role.clone()))
}

pub fn has_role(env: &Env, role: &Symbol, account: &Address) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::RoleGrant(role.clone(), account.clone()))
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn set_role_grant(env: &Env, role: &Symbol, account: &Address, granted: bool) {
    let key = StorageKey::RoleGrant(role.clone(), account.clone());
    if granted {
        env.storage().instance().set(&key, &true);
    } else {
        env.storage().instance().remove(&key);
    }
}

#[allow(dead_code)]
pub fn get_fee_tiers(env: &Env) -> Option<Vec<FeeTier>> {
    env.storage().instance().get(&StorageKey::FeeTiers)
}

#[allow(dead_code)]
pub fn set_fee_tiers(env: &Env, tiers: &Vec<FeeTier>) {
    env.storage().instance().set(&StorageKey::FeeTiers, tiers);
}

pub fn get_investor_bonus_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::InvestorBonusBps)
        .unwrap_or(0)
}

pub fn set_investor_bonus_bps(env: &Env, bonus_bps: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::InvestorBonusBps, &bonus_bps);
}
