//! Storage helpers for balances, metadata, allowances, fees, roles, nonces, and history.

use soroban_sdk::{Address, Symbol, Vec};

use crate::types::{AllowanceData, OwnershipHistoryRecord, StorageKey, TokenMetadata};

/// Load token metadata from instance storage.
pub fn get_metadata(env: &soroban_sdk::Env) -> Option<TokenMetadata> {
    env.storage().instance().get(&StorageKey::Metadata)
}

/// Save token metadata to instance storage.
pub fn set_metadata(env: &soroban_sdk::Env, meta: &TokenMetadata) {
    env.storage().instance().set(&StorageKey::Metadata, meta);
}

/// Load total supply from instance storage.
pub fn get_total_supply(env: &soroban_sdk::Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalSupply)
        .unwrap_or(0)
}

/// Save total supply to instance storage.
pub fn set_total_supply(env: &soroban_sdk::Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalSupply, &amount);
}

/// Get balance for an address (persistent storage).
/// Auto-extends the storage key TTL on access to prevent expiration during active use.
pub fn get_balance(env: &soroban_sdk::Env, addr: &Address) -> i128 {
    let key = StorageKey::Balance(addr.clone());
    let balance = env.storage().persistent().get(&key).unwrap_or(0);

    // Extend TTL to prevent key expiration during active use
    let current_ledger = env.ledger().sequence();
    env.storage()
        .persistent()
        .extend_ttl(&key, current_ledger + 1_000, current_ledger + 1_000);

    balance
}

/// Set balance for an address (persistent storage).
pub fn set_balance(env: &soroban_sdk::Env, addr: &Address, amount: i128) {
    if amount == 0 {
        env.storage()
            .persistent()
            .remove(&StorageKey::Balance(addr.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&StorageKey::Balance(addr.clone()), &amount);
    }
}

/// Check whether an account is frozen.
pub fn is_account_frozen(env: &soroban_sdk::Env, account: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::Frozen(account.clone()))
        .unwrap_or(false)
}

/// Update an account's frozen state, removing unrestricted entries from storage.
pub fn set_account_frozen(env: &soroban_sdk::Env, account: &Address, frozen: bool) {
    let key = StorageKey::Frozen(account.clone());
    if frozen {
        env.storage().persistent().set(&key, &true);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// Get allowance (from, spender). Returns 0 if expired or not set.
pub fn get_allowance(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    current_ledger: u32,
) -> i128 {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    let data: Option<AllowanceData> = env.storage().persistent().get(&key);
    match data {
        Some(a) if a.expiration_ledger >= current_ledger => a.amount,
        _ => 0,
    }
}

/// Set allowance (from, spender) -> (amount, expiration_ledger).
/// Removes the key when amount is 0 to save persistent storage.
pub fn set_allowance(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(
            &key,
            &AllowanceData {
                amount,
                expiration_ledger,
            },
        );
    }
}

/// Get raw allowance data (for decreasing allowance on transfer_from/burn_from).
pub fn get_allowance_data(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
) -> Option<AllowanceData> {
    env.storage()
        .persistent()
        .get(&StorageKey::Allowance(from.clone(), spender.clone()))
}

// ==================== Fee (Issue #113) ====================

/// Get fee basis points from instance storage. Returns 0 if not set.
pub fn get_fee_bps(env: &soroban_sdk::Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::FeeBps)
        .unwrap_or(0)
}

/// Save fee basis points to instance storage.
pub fn set_fee_bps(env: &soroban_sdk::Env, bps: i128) {
    env.storage().instance().set(&StorageKey::FeeBps, &bps);
}

// ==================== Role Admin (Issue #108) ====================

/// Get the admin address for a specific role. Returns None if unset.
pub fn get_role_admin(env: &soroban_sdk::Env, role: &Symbol) -> Option<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::RoleAdmin(role.clone()))
}

/// Set the admin address for a specific role.
pub fn set_role_admin(env: &soroban_sdk::Env, role: &Symbol, admin: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::RoleAdmin(role.clone()), admin);
}

/// Check whether `account` has been granted `role`.
pub fn has_role(env: &soroban_sdk::Env, role: &Symbol, account: &Address) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::RoleGrant(role.clone(), account.clone()))
        .unwrap_or(false)
}

/// Grant or revoke `role` for `account`.
pub fn set_role_grant(env: &soroban_sdk::Env, role: &Symbol, account: &Address, granted: bool) {
    let key = StorageKey::RoleGrant(role.clone(), account.clone());
    if granted {
        env.storage().instance().set(&key, &true);
    } else {
        env.storage().instance().remove(&key);
    }
}

// ==================== Nonce (Issue #106) ====================

/// Get the current nonce for an address. Starts at 0.
pub fn get_nonce(env: &soroban_sdk::Env, addr: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&StorageKey::Nonce(addr.clone()))
        .unwrap_or(0u64)
}

/// Increment and return the new nonce for an address.
#[allow(dead_code)]
pub fn increment_nonce(env: &soroban_sdk::Env, addr: &Address) -> u64 {
    let current = get_nonce(env, addr);
    let new_nonce = current + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::Nonce(addr.clone()), &new_nonce);
    new_nonce
}

// ==================== Ownership History (Issue #111) ====================

/// Get the full ownership history for an address. Returns empty Vec if none.
pub fn get_token_history(env: &soroban_sdk::Env, addr: &Address) -> Vec<OwnershipHistoryRecord> {
    env.storage()
        .persistent()
        .get(&StorageKey::History(addr.clone()))
        .unwrap_or_else(|| soroban_sdk::Vec::new(env))
}

/// Append an ownership history record for an address.
pub fn append_token_history(
    env: &soroban_sdk::Env,
    addr: &Address,
    record: &OwnershipHistoryRecord,
) {
    let key = StorageKey::History(addr.clone());
    let mut history: Vec<OwnershipHistoryRecord> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env));
    history.push_back(record.clone());
    env.storage().persistent().set(&key, &history);
}

// ==================== Allowance Expiration Extension ====================

/// Extend the expiration ledger of an existing allowance in place, leaving its amount untouched.
pub fn extend_allowance_expiration(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    new_expiration_ledger: u32,
) {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    if let Some(mut data) = env.storage().persistent().get::<_, AllowanceData>(&key) {
        data.expiration_ledger = new_expiration_ledger;
        env.storage().persistent().set(&key, &data);
    }
}
