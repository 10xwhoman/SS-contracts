//! Invoice Token (SEP-41) contract for StellarSettle.
//!
//! Implements a fungible token representing fractional ownership of an invoice,
//! with minting, burning, allowances, optional transfer lock,
//! fee deduction, role-based admin, nonce tracking, and ownership history.

#![no_std]

mod constants;
mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Symbol, Vec};

use crate::errors::Error;
use crate::types::{is_zero_address, OwnershipHistoryRecord, TokenMetadata, MAX_DECIMALS};
const ADMIN_ROLE: &str = "admin";
const MINTER_ROLE: &str = "minter";
const PAUSER_ROLE: &str = "pauser";
const TRANSFER_LOCKER_ROLE: &str = "transfer_locker";

fn ensure_account_not_frozen(env: &Env, account: &Address) -> Result<(), Error> {
    if storage::is_account_frozen(env, account) {
        return Err(Error::AccountFrozen);
    }
    Ok(())
}

fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    if is_zero_address(env, address) {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

fn ensure_non_zero_addresses<T, I>(env: &Env, addresses: I) -> Result<(), Error>
where
    T: core::borrow::Borrow<Address>,
    I: IntoIterator<Item = T>,
{
    for address in addresses {
        ensure_non_zero_address(env, core::borrow::Borrow::borrow(&address))?;
    }
    Ok(())
}

#[contract]
pub struct InvoiceToken;

#[contractimpl]
impl InvoiceToken {
    /// Initialize the token with admin, metadata, and minter (escrow) address.
    pub fn initialize(
        env: Env,
        admin: Address,
        name: SorobanString,
        symbol: SorobanString,
        decimals: u32,
        invoice_id: Symbol,
        minter: Address,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&admin, &minter])?;
        if storage::get_metadata(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if decimals > MAX_DECIMALS {
            return Err(Error::InvalidDecimals);
        }
        // SEP-41 metadata (name, symbol) must be meaningful, not empty placeholders.
        if name.is_empty() || symbol.is_empty() {
            return Err(Error::InvalidMetadata);
        }
        let meta = TokenMetadata {
            admin: admin.clone(),
            minter: minter.clone(),
            name,
            symbol,
            decimals,
            invoice_id,
            transfer_locked: true, // default locked until settlement
            paused: false,
        };
        storage::set_metadata(&env, &meta);
        storage::set_total_supply(&env, 0);

        // Seed default roles: admin gets all roles, minter gets minter role.
        let admin_role_sym = Symbol::new(&env, ADMIN_ROLE);
        let minter_role_sym = Symbol::new(&env, MINTER_ROLE);
        let pauser_role_sym = Symbol::new(&env, PAUSER_ROLE);
        let tlock_role_sym = Symbol::new(&env, TRANSFER_LOCKER_ROLE);

        // Admin is the default admin for every role.
        for role in [
            &admin_role_sym,
            &minter_role_sym,
            &pauser_role_sym,
            &tlock_role_sym,
        ] {
            storage::set_role_admin(&env, role, &admin);
            storage::set_role_grant(&env, role, &admin, true);
        }

        // Grant minter role to minter address.
        storage::set_role_grant(&env, &minter_role_sym, &minter, true);

        Ok(())
    }

    // ---------- SEP-41 standard view functions ----------

    pub fn name(env: Env) -> Result<SorobanString, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.name)
    }

    pub fn symbol(env: Env) -> Result<SorobanString, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.symbol)
    }

    pub fn decimals(env: Env) -> Result<u32, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.decimals)
    }

    pub fn total_supply(env: Env) -> Result<i128, Error> {
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_total_supply(&env))
    }

    pub fn balance(env: Env, id: Address) -> Result<i128, Error> {
        ensure_non_zero_address(&env, &id)?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_balance(&env, &id))
    }

    /// Return balances for a batch of addresses, preserving the input order.
    pub fn balance_batch(env: Env, ids: Vec<Address>) -> Result<Vec<i128>, Error> {
        ensure_non_zero_addresses(&env, ids.iter())?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let mut balances = Vec::new(&env);
        for id in ids.iter() {
            balances.push_back(storage::get_balance(&env, &id));
        }
        Ok(balances)
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> Result<i128, Error> {
        ensure_non_zero_addresses(&env, [&from, &spender])?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let ledger = env.ledger().sequence();
        Ok(storage::get_allowance(&env, &from, &spender, ledger))
    }

    // ---------- Issue #106: Nonce query ----------

    /// Returns the current nonce for the given address (used in EIP-2612-style permits).
    pub fn get_nonce(env: Env, account: Address) -> Result<u64, Error> {
        ensure_non_zero_address(&env, &account)?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let nonce = storage::get_nonce(&env, &account);
        events::nonce_queried_event(&env, &account, nonce);
        Ok(nonce)
    }

    // ---------- SEP-41 transfer (with Issue #113 fee deduction) ----------

    /// Transfer amount from `from` to `to`. Requires `from` auth.
    /// When a fee is configured, the fee is deducted from `from` and sent to the admin.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&from, &to])?;
        from.require_auth();
        if from == to {
            return Err(Error::InvalidAddress);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &from)?;
        ensure_account_not_frozen(&env, &to)?;
        if meta.transfer_locked && from != meta.admin {
            return Err(Error::TransferLocked);
        }

        let fee_bps = storage::get_fee_bps(&env);
        let fee_amount = if fee_bps > 0 {
            amount
                .checked_mul(fee_bps)
                .and_then(|v| v.checked_div(10_000))
                .ok_or(Error::Overflow)?
        } else {
            0
        };
        if fee_amount > 0 {
            ensure_account_not_frozen(&env, &meta.admin)?;
        }

        let total_debit = amount.checked_add(fee_amount).ok_or(Error::Overflow)?;

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < total_debit {
            if fee_amount > 0 && from_balance >= amount && from_balance < total_debit {
                return Err(Error::InsufficientBalanceForFee);
            }
            return Err(Error::InsufficientBalance);
        }

        storage::set_balance(&env, &from, from_balance - total_debit);
        let to_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, to_balance);

        // Send fee to admin if applicable.
        if fee_amount > 0 {
            let admin_balance = storage::get_balance(&env, &meta.admin)
                .checked_add(fee_amount)
                .ok_or(Error::Overflow)?;
            storage::set_balance(&env, &meta.admin, admin_balance);
            events::fee_deducted_event(&env, &from, fee_amount);
        }

        // Record ownership history for both sender and receiver.
        let ledger = env.ledger().sequence();
        let record = OwnershipHistoryRecord {
            from: from.clone(),
            to: to.clone(),
            amount,
            ledger,
        };
        storage::append_token_history(&env, &from, &record);
        storage::append_token_history(&env, &to, &record);
        events::history_appended_event(&env, &from, &from, &to, amount);

        events::transfer_event(&env, &from, &to, amount);
        Ok(())
    }

    // ---------- SEP-41 allowance ----------

    /// Set allowance for spender. Requires `from` auth.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&from, &spender])?;
        from.require_auth();
        if from == spender {
            return Err(Error::InvalidAddress);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &from)?;
        ensure_account_not_frozen(&env, &spender)?;
        let ledger = env.ledger().sequence();
        if amount != 0 && expiration_ledger < ledger {
            return Err(Error::InvalidExpiration);
        }
        storage::set_allowance(&env, &from, &spender, amount, expiration_ledger);
        events::approve_event(&env, &from, &spender, amount, expiration_ledger);
        Ok(())
    }

    /// Extend the expiration ledger of an existing allowance without changing its amount.
    /// Requires `from` auth. The new expiration must be strictly later than the current one,
    /// and the allowance must not already be expired (use `approve` to reinstate an expired one).
    pub fn extend_allowance(
        env: Env,
        from: Address,
        spender: Address,
        new_expiration_ledger: u32,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&from, &spender])?;
        from.require_auth();
        if from == spender {
            return Err(Error::InvalidAddress);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &from)?;
        ensure_account_not_frozen(&env, &spender)?;
        let allow =
            storage::get_allowance_data(&env, &from, &spender).ok_or(Error::AllowanceNotFound)?;
        let ledger = env.ledger().sequence();
        if allow.expiration_ledger < ledger {
            return Err(Error::AllowanceExpired);
        }
        if new_expiration_ledger <= allow.expiration_ledger {
            return Err(Error::InvalidExpiration);
        }
        storage::extend_allowance_expiration(&env, &from, &spender, new_expiration_ledger);
        events::allowance_extended_event(&env, &from, &spender, new_expiration_ledger);
        Ok(())
    }

    /// Revoke `spender`'s allowance. Requires `from` authorization.
    pub fn revoke_approval(env: Env, from: Address, spender: Address) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&from, &spender])?;
        from.require_auth();
        if from == spender {
            return Err(Error::InvalidAddress);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        storage::set_allowance(&env, &from, &spender, 0, 0);
        events::approval_revoked_event(&env, &from, &spender);
        Ok(())
    }

    /// Transfer from `from` to `to` using allowance. Requires `spender` auth.
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&spender, &from, &to])?;
        spender.require_auth();
        if from == to {
            return Err(Error::InvalidAddress);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &spender)?;
        ensure_account_not_frozen(&env, &from)?;
        ensure_account_not_frozen(&env, &to)?;
        if meta.transfer_locked && from != meta.admin {
            return Err(Error::TransferLocked);
        }
        let ledger = env.ledger().sequence();
        let allow = storage::get_allowance_data(&env, &from, &spender)
            .ok_or(Error::InsufficientAllowance)?;
        if allow.expiration_ledger < ledger {
            return Err(Error::AllowanceExpired);
        }
        if allow.amount < amount {
            return Err(Error::InsufficientAllowance);
        }

        let fee_bps = storage::get_fee_bps(&env);
        let fee_amount = if fee_bps > 0 {
            amount
                .checked_mul(fee_bps)
                .and_then(|v| v.checked_div(10_000))
                .ok_or(Error::Overflow)?
        } else {
            0
        };
        if fee_amount > 0 {
            ensure_account_not_frozen(&env, &meta.admin)?;
        }

        let total_debit = amount.checked_add(fee_amount).ok_or(Error::Overflow)?;

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < total_debit {
            if fee_amount > 0 && from_balance >= amount && from_balance < total_debit {
                return Err(Error::InsufficientBalanceForFee);
            }
            return Err(Error::InsufficientBalance);
        }

        storage::set_allowance(
            &env,
            &from,
            &spender,
            allow.amount - amount,
            allow.expiration_ledger,
        );
        storage::set_balance(&env, &from, from_balance - total_debit);
        let to_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, to_balance);

        if fee_amount > 0 {
            let admin_balance = storage::get_balance(&env, &meta.admin)
                .checked_add(fee_amount)
                .ok_or(Error::Overflow)?;
            storage::set_balance(&env, &meta.admin, admin_balance);
            events::fee_deducted_event(&env, &from, fee_amount);
        }

        // Record ownership history for both sender and receiver.
        let record = OwnershipHistoryRecord {
            from: from.clone(),
            to: to.clone(),
            amount,
            ledger,
        };
        storage::append_token_history(&env, &from, &record);
        storage::append_token_history(&env, &to, &record);
        events::history_appended_event(&env, &from, &from, &to, amount);

        events::transfer_event(&env, &from, &to, amount);
        Ok(())
    }

    // ---------- SEP-41 burn ----------

    /// Burn amount from `from`. Requires `from` auth.
    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        ensure_non_zero_address(&env, &from)?;
        from.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &from)?;
        let balance = storage::get_balance(&env, &from);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_balance(&env, &from, balance - amount);
        let new_supply = storage::get_total_supply(&env)
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        events::burn_event(&env, &from, amount);
        Ok(())
    }

    /// Burn from `from` using spender's allowance. Requires `spender` auth.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&spender, &from])?;
        spender.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        ensure_account_not_frozen(&env, &spender)?;
        ensure_account_not_frozen(&env, &from)?;
        let ledger = env.ledger().sequence();
        let allow = storage::get_allowance_data(&env, &from, &spender)
            .ok_or(Error::InsufficientAllowance)?;
        if allow.expiration_ledger < ledger {
            return Err(Error::AllowanceExpired);
        }
        if allow.amount < amount {
            return Err(Error::InsufficientAllowance);
        }
        let balance = storage::get_balance(&env, &from);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_allowance(
            &env,
            &from,
            &spender,
            allow.amount - amount,
            allow.expiration_ledger,
        );
        storage::set_balance(&env, &from, balance - amount);
        let new_supply = storage::get_total_supply(&env)
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        events::burn_event(&env, &from, amount);
        Ok(())
    }

    // ---------- Admin / minter ----------

    /// Mint tokens to `to`. Callable only by admin or minter (escrow).
    /// `by` must be admin or minter and must authorize the call.
    pub fn mint(env: Env, to: Address, amount: i128, by: Address) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&to, &by])?;
        by.require_auth();
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        if by != meta.admin && by != meta.minter {
            return Err(Error::Unauthorized);
        }
        ensure_account_not_frozen(&env, &by)?;
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        ensure_account_not_frozen(&env, &to)?;
        let new_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        let new_supply = storage::get_total_supply(&env)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, new_balance);
        storage::set_total_supply(&env, new_supply);
        events::mint_event(&env, &to, amount);
        Ok(())
    }

    /// Mint tokens to multiple addresses in a batch. Callable only by admin or minter.
    /// `to` and `amounts` vectors must be of equal length. Each amount must be > 0.
    pub fn mint_batch(
        env: Env,
        to: Vec<Address>,
        amounts: Vec<i128>,
        by: Address,
    ) -> Result<(), Error> {
        ensure_non_zero_address(&env, &by)?;
        ensure_non_zero_addresses(&env, to.iter())?;
        by.require_auth();
        if to.len() != amounts.len() {
            return Err(Error::BatchLengthMismatch);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        if by != meta.admin && by != meta.minter {
            return Err(Error::Unauthorized);
        }
        ensure_account_not_frozen(&env, &by)?;
        // Validate amounts and compute total
        let mut total_amount: i128 = 0;
        for i in 0..amounts.len() {
            let amount = amounts.get(i).unwrap();
            if amount < 0 {
                return Err(Error::InvalidAmount);
            }
            if amount == 0 {
                continue;
            }
            let recipient = to.get(i).unwrap();
            ensure_account_not_frozen(&env, &recipient)?;
            total_amount = total_amount.checked_add(amount).ok_or(Error::Overflow)?;
            let new_bal = storage::get_balance(&env, &recipient)
                .checked_add(amount)
                .ok_or(Error::Overflow)?;
            storage::set_balance(&env, &recipient, new_bal);
            events::mint_event(&env, &recipient, amount);
        }
        // Update total supply once
        let new_total_supply = storage::get_total_supply(&env)
            .checked_add(total_amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_total_supply);
        Ok(())
    }

    /// Set transfer lock. Callable by admin or minter (escrow contract).
    /// When true, only admin can transfer; when false, all holders can transfer.
    pub fn set_transfer_locked(env: Env, caller: Address, locked: bool) -> Result<(), Error> {
        ensure_non_zero_address(&env, &caller)?;
        caller.require_auth();
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if caller != meta.admin && caller != meta.minter {
            return Err(Error::Unauthorized);
        }
        let old_locked = meta.transfer_locked;
        meta.transfer_locked = locked;
        storage::set_metadata(&env, &meta);
        events::transfer_locked_updated_event(&env, old_locked, locked);
        Ok(())
    }

    /// Set minter address (admin only).
    pub fn set_minter(env: Env, new_minter: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &new_minter)?;
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let old_minter = meta.minter.clone();
        meta.admin.require_auth();
        meta.minter = new_minter.clone();
        storage::set_metadata(&env, &meta);

        // Update minter role grant: revoke from old, grant to new.
        let minter_role = Symbol::new(&env, MINTER_ROLE);
        storage::set_role_grant(&env, &minter_role, &old_minter, false);
        storage::set_role_grant(&env, &minter_role, &new_minter, true);

        events::minter_updated_event(&env, &old_minter, &meta.minter);
        Ok(())
    }

    /// Update the token name. Admin only.
    pub fn set_name(env: Env, new_name: SorobanString) -> Result<(), Error> {
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        if new_name.is_empty() {
            return Err(Error::InvalidMetadata);
        }
        meta.name = new_name;
        storage::set_metadata(&env, &meta);
        Ok(())
    }

    /// Update the token symbol. Admin only.
    pub fn set_symbol(env: Env, new_symbol: SorobanString) -> Result<(), Error> {
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        if new_symbol.is_empty() {
            return Err(Error::InvalidMetadata);
        }
        meta.symbol = new_symbol;
        storage::set_metadata(&env, &meta);
        Ok(())
    }

    /// Update the fractional precision for this invoice sub-asset. Admin only.
    pub fn set_decimals(env: Env, decimals: u32) -> Result<(), Error> {
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        if decimals > MAX_DECIMALS {
            return Err(Error::InvalidDecimals);
        }
        let old_decimals = meta.decimals;
        meta.decimals = decimals;
        storage::set_metadata(&env, &meta);
        events::decimals_updated_event(&env, old_decimals, decimals);
        Ok(())
    }

    /// Set emergency pause state. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        let old_paused = meta.paused;
        meta.paused = paused;
        storage::set_metadata(&env, &meta);
        events::paused_updated_event(&env, old_paused, paused);
        Ok(())
    }

    /// Get invoice_id for this token (metadata).
    pub fn invoice_id(env: Env) -> Result<Symbol, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.invoice_id)
    }

    /// Check if transfers are locked.
    pub fn transfer_locked(env: Env) -> Result<bool, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.transfer_locked)
    }

    /// Check if the contract is paused.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.paused)
    }

    // ---------- Account restrictions ----------

    /// Freeze an account. Admin only.
    pub fn freeze_account(env: Env, account: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &account)?;
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        if storage::is_account_frozen(&env, &account) {
            return Err(Error::AccountFrozen);
        }
        storage::set_account_frozen(&env, &account, true);
        events::account_frozen_event(&env, &account);
        Ok(())
    }

    /// Unfreeze an account. Admin only.
    pub fn unfreeze_account(env: Env, account: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &account)?;
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        if !storage::is_account_frozen(&env, &account) {
            return Err(Error::AccountNotFrozen);
        }
        storage::set_account_frozen(&env, &account, false);
        events::account_unfrozen_event(&env, &account);
        Ok(())
    }

    /// Clawback tokens from a frozen account. Admin only. SEP-41 compliance.
    /// This function allows the admin to reclaim tokens from a frozen account
    /// for regulatory compliance purposes (e.g., court orders, sanctions, fraud).
    /// The clawback destination can be the admin or any other designated address.
    pub fn clawback(
        env: Env,
        from: Address,
        amount: i128,
    ) -> Result<(), Error> {
        ensure_non_zero_address(&env, &from)?;
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        
        // SEP-41: Clawback can only be performed on frozen accounts
        if !storage::is_account_frozen(&env, &from) {
            return Err(Error::AccountNotFrozen);
        }
        
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        
        let balance = storage::get_balance(&env, &from);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        
        // Reduce the frozen account's balance
        storage::set_balance(&env, &from, balance - amount);
        
        // Reduce total supply (tokens are effectively burned)
        let new_supply = storage::get_total_supply(&env)
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        
        // Emit event for regulatory audit trail
        events::clawback_event(&env, &from, amount);
        Ok(())
    }

    /// Check whether an account is frozen.
    pub fn is_account_frozen(env: Env, account: Address) -> Result<bool, Error> {
        ensure_non_zero_address(&env, &account)?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::is_account_frozen(&env, &account))
    }

    /// Check whether an account is frozen.
    pub fn is_frozen(env: Env, account: Address) -> Result<bool, Error> {
        Self::is_account_frozen(env, account)
    }

    // ---------- Issue #113: Fee management ----------

    /// Get current fee in basis points (0 = no fee, 100 = 1%).
    pub fn get_fee_bps(env: Env) -> Result<i128, Error> {
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_fee_bps(&env))
    }

    /// Set fee basis points. Admin only. Valid range: 0..=10_000.
    pub fn set_fee_bps(env: Env, caller: Address, new_bps: i128) -> Result<(), Error> {
        ensure_non_zero_address(&env, &caller)?;
        caller.require_auth();
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if caller != meta.admin {
            return Err(Error::Unauthorized);
        }
        if !(0..=10_000).contains(&new_bps) {
            return Err(Error::InvalidFeeBps);
        }
        let old_bps = storage::get_fee_bps(&env);
        storage::set_fee_bps(&env, new_bps);
        events::fee_updated_event(&env, old_bps, new_bps);
        Ok(())
    }

    // ---------- Issue #108: Role-based admin ----------

    /// Get the admin address for a role (who can grant/revoke that role).
    pub fn get_role_admin(env: Env, role: Symbol) -> Result<Address, Error> {
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        storage::get_role_admin(&env, &role).ok_or(Error::RoleNotGranted)
    }

    /// Set the admin for a role. The caller must be the current admin of that role.
    pub fn set_role_admin(
        env: Env,
        caller: Address,
        role: Symbol,
        new_admin: Address,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&caller, &new_admin])?;
        caller.require_auth();
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let current_admin = storage::get_role_admin(&env, &role).ok_or(Error::RoleNotGranted)?;
        if caller != current_admin {
            return Err(Error::Unauthorized);
        }
        let old_admin = current_admin;
        storage::set_role_admin(&env, &role, &new_admin);
        events::role_admin_updated_event(&env, &role, &old_admin, &new_admin);
        Ok(())
    }

    /// Grant a role to an account. Caller must be the admin of that role.
    pub fn grant_role(
        env: Env,
        caller: Address,
        role: Symbol,
        account: Address,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&caller, &account])?;
        caller.require_auth();
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let role_admin = storage::get_role_admin(&env, &role).ok_or(Error::RoleNotGranted)?;
        if caller != role_admin {
            return Err(Error::Unauthorized);
        }
        storage::set_role_grant(&env, &role, &account, true);
        events::role_granted_event(&env, &role, &account, true);
        Ok(())
    }

    /// Revoke a role from an account. Caller must be the admin of that role.
    pub fn revoke_role(
        env: Env,
        caller: Address,
        role: Symbol,
        account: Address,
    ) -> Result<(), Error> {
        ensure_non_zero_addresses(&env, [&caller, &account])?;
        caller.require_auth();
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let role_admin = storage::get_role_admin(&env, &role).ok_or(Error::RoleNotGranted)?;
        if caller != role_admin {
            return Err(Error::Unauthorized);
        }
        storage::set_role_grant(&env, &role, &account, false);
        events::role_granted_event(&env, &role, &account, false);
        Ok(())
    }

    /// Check whether an account has been granted a role.
    pub fn has_role(env: Env, role: Symbol, account: Address) -> Result<bool, Error> {
        ensure_non_zero_address(&env, &account)?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::has_role(&env, &role, &account))
    }

    // ---------- Issue #111: Ownership history ----------

    /// Get the full ownership history for a given address.
    pub fn get_token_history(
        env: Env,
        account: Address,
    ) -> Result<soroban_sdk::Vec<OwnershipHistoryRecord>, Error> {
        ensure_non_zero_address(&env, &account)?;
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_token_history(&env, &account))
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod sep41_compliance;
#[cfg(test)]
mod test;
