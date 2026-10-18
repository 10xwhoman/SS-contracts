//! Waitlist contract for StellarSettle.
//!
//! Stores pre-approved investor wallets ahead of their KYC approval and
//! automatically promotes them to the KYC whitelist (the invoice-escrow buyer
//! whitelist) when their access slot is granted.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol};

use errors::Error;
use types::Config;

const SET_WHITELISTED_FN: &str = "set_buyer_whitelisted";

#[contract]
pub struct Waitlist;

/// Promote a wallet to the KYC whitelist by calling the escrow contract's
/// `set_buyer_whitelisted` entrypoint. Returns an error if the cross-contract
/// call fails.
fn promote_to_whitelist(
    env: &Env,
    escrow: &Address,
    admin: &Address,
    wallet: &Address,
) -> Result<(), Error> {
    let result = env.try_invoke_contract::<(), soroban_sdk::Error>(
        escrow,
        &Symbol::new(env, SET_WHITELISTED_FN),
        soroban_sdk::vec![env, admin.to_val(), wallet.to_val(), true.into_val(env),],
    );
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(Error::WhitelistCallFailed),
        Err(_) => Err(Error::WhitelistCallFailed),
    }
}

#[contractimpl]
impl Waitlist {
    /// Initialize the contract with admin and the KYC whitelist (escrow) contract.
    pub fn initialize(env: Env, admin: Address, escrow_contract: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        let config = Config {
            admin,
            escrow_contract,
        };
        storage::set_config(&env, &config);
        storage::set_next_slot(&env, 1);
        Ok(())
    }

    /// Open registration: add a wallet to the waitlist queue.
    /// The wallet itself must authorize. Stores the wallet at the next
    /// available queue position along with an email hash for record-keeping.
    pub fn join_waitlist(env: Env, wallet: Address, email_hash: BytesN<32>) -> Result<(), Error> {
        wallet.require_auth();
        let _config = storage::get_config(&env).ok_or(Error::NotInit)?;

        if storage::get_wallet_pos(&env, &wallet) != 0 {
            return Err(Error::AlreadyOnWaitlist);
        }

        let position = storage::get_next_slot(&env);
        storage::set_wallet_at(&env, position, &wallet);
        storage::set_wallet_pos(&env, &wallet, position);
        storage::set_wallet_email(&env, &wallet, &email_hash);
        storage::set_next_slot(&env, position + 1);

        events::waitlist_joined(&env, &wallet, position);
        Ok(())
    }

    /// Admin-only: approve a single wallet and promote it to the KYC whitelist.
    /// Emits `WaitlistApproved` with the wallet and current timestamp.
    pub fn approve_from_waitlist(env: Env, wallet: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        if storage::get_wallet_pos(&env, &wallet) == 0 {
            return Err(Error::NotOnWaitlist);
        }
        if storage::is_approved(&env, &wallet) {
            return Err(Error::AlreadyApproved);
        }

        promote_to_whitelist(&env, &config.escrow_contract, &config.admin, &wallet)?;

        storage::set_approved(&env, &wallet, true);
        events::waitlist_approved(&env, &wallet, env.ledger().timestamp());
        Ok(())
    }

    /// Admin-only: approve the top N wallets from the queue that have not yet
    /// been approved. Wallets are processed in queue order (position 1 first).
    /// Emits `WaitlistApproved` for each approved wallet.
    pub fn bulk_approve(env: Env, count: u32) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        if count == 0 {
            return Err(Error::InvalidCount);
        }

        let next_slot = storage::get_next_slot(&env);
        let mut approved_count: u32 = 0;

        for pos in 1..next_slot {
            if approved_count >= count {
                break;
            }
            if let Some(wallet) = storage::get_wallet_at(&env, pos) {
                if !storage::is_approved(&env, &wallet) {
                    promote_to_whitelist(&env, &config.escrow_contract, &config.admin, &wallet)?;
                    storage::set_approved(&env, &wallet, true);
                    events::waitlist_approved(&env, &wallet, env.ledger().timestamp());
                    approved_count += 1;
                }
            }
        }

        if approved_count == 0 {
            return Err(Error::InvalidCount);
        }

        Ok(())
    }

    /// View: return the current queue position for a wallet.
    /// Returns 0 if the wallet is not on the waitlist.
    pub fn get_position(env: Env, wallet: Address) -> Result<u32, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_wallet_pos(&env, &wallet))
    }

    /// View: return whether a wallet has been approved (promoted to KYC whitelist).
    pub fn is_approved(env: Env, wallet: Address) -> Result<bool, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::is_approved(&env, &wallet))
    }

    /// View: return the current config.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return the total number of wallets on the waitlist.
    pub fn get_waitlist_size(env: Env) -> Result<u32, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_next_slot(&env).saturating_sub(1))
    }
}

#[cfg(test)]
mod test;
