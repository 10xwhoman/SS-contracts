//! Referral contract for StellarSettle.
//!
//! Tracks referral relationships between investors, accrues rewards when
//! referees make investments, and allows referrers to claim accumulated payouts.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

use errors::Error;
use types::{Config, ReferralStats};

const MAX_BPS: u32 = 10_000;

#[contract]
pub struct Referral;

#[contractimpl]
impl Referral {
    /// Initialize the contract with admin, fee rate, and payment token.
    pub fn initialize(env: Env, admin: Address, fee_bps: u32, token: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let config = Config {
            admin,
            fee_bps,
            token,
            escrow: None,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Register a referral: bind `referee` to `referrer`. One-time per wallet.
    /// The referee authorizes the registration. A wallet that already has a
    /// referrer is rejected with `AlreadyReferred`.
    pub fn register_referral(env: Env, referee: Address, referrer: Address) -> Result<(), Error> {
        referee.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let _ = &config;

        if referee == referrer {
            return Err(Error::SelfReferral);
        }

        if storage::get_referrer(&env, &referee).is_some() {
            return Err(Error::AlreadyReferred);
        }

        storage::set_referrer(&env, &referee, &referrer);

        // Increment the referrer's referee count.
        let count = storage::get_ref_count(&env, &referrer);
        storage::set_ref_count(&env, &referrer, count + 1);

        events::referral_registered(&env, &referee, &referrer);
        Ok(())
    }

    /// Accrue a referral fee for a referee's investment. Called by the escrow
    /// contract on each investment. The fee is `amount * fee_bps / 10000`,
    /// added to the referrer's pending balance.
    ///
    /// `escrow` is the calling contract's own address. It must authorize the
    /// call (a contract authorizes its direct sub-invocations, so the escrow
    /// calling this function satisfies the requirement).
    ///
    /// If the referee has no referrer, this is a no-op (returns Ok).
    /// If an escrow contract address is bound via `set_escrow`, only that
    /// exact escrow address may accrue fees.
    pub fn accrue_referral_fee(
        env: Env,
        escrow: Address,
        invoice_id: Symbol,
        referee: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;

        // Enforce optional escrow whitelist: once bound, only that exact
        // address may accrue fees.
        if let Some(bound) = config.escrow.as_ref() {
            if &escrow != bound {
                return Err(Error::Unauthorized);
            }
        }

        // The caller (or an account that signed on its behalf) must authorize.
        escrow.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Look up the referee's referrer. No referrer = no-op.
        let referrer = match storage::get_referrer(&env, &referee) {
            Some(r) => r,
            None => return Ok(()),
        };

        // Compute fee: amount * fee_bps / 10000
        let fee = amount
            .checked_mul(i128::from(config.fee_bps))
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;

        if fee <= 0 {
            return Ok(());
        }

        // Accrue to pending and total earned.
        let pending = storage::get_pending(&env, &referrer);
        let new_pending = pending.checked_add(fee).ok_or(Error::Overflow)?;
        storage::set_pending(&env, &referrer, new_pending);

        let total = storage::get_total_earned(&env, &referrer);
        let new_total = total.checked_add(fee).ok_or(Error::Overflow)?;
        storage::set_total_earned(&env, &referrer, new_total);

        events::referral_fee_paid(&env, invoice_id, &referee, &referrer, fee);
        Ok(())
    }

    /// Claim all pending rewards. Transfers the accumulated token balance
    /// to the referrer and resets the pending amount to zero.
    pub fn claim_rewards(env: Env, referrer: Address) -> Result<(), Error> {
        referrer.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;

        let pending = storage::get_pending(&env, &referrer);
        if pending <= 0 {
            return Err(Error::NothingToClaim);
        }

        let token_client = token::Client::new(&env, &config.token);
        let contract_addr = env.current_contract_address();

        let balance = token_client.balance(&contract_addr);
        if balance < pending {
            return Err(Error::InsufficientBalance);
        }

        token_client.transfer(&contract_addr, &referrer, &pending);
        storage::set_pending(&env, &referrer, 0);

        events::rewards_claimed(&env, &referrer, pending);
        Ok(())
    }

    /// View: return referral statistics for a referrer.
    pub fn get_referral_stats(env: Env, referrer: Address) -> Result<ReferralStats, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(ReferralStats {
            total_earned: storage::get_total_earned(&env, &referrer),
            pending: storage::get_pending(&env, &referrer),
            referee_count: storage::get_ref_count(&env, &referrer),
        })
    }

    /// Admin-only: update the referral fee rate (basis points).
    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let old_bps = config.fee_bps;
        config.fee_bps = fee_bps;
        storage::set_config(&env, &config);
        events::fee_rate_updated(&env, old_bps, fee_bps);
        Ok(())
    }

    /// Admin-only: bind the escrow contract allowed to call accrue_referral_fee.
    /// When set, only that escrow may accrue fees. Pass None to clear.
    pub fn set_escrow(env: Env, admin: Address, escrow: Option<Address>) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.escrow = escrow;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// View: return the referrer for a given referee, if registered.
    pub fn get_referrer(env: Env, referee: Address) -> Result<Option<Address>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_referrer(&env, &referee))
    }

    /// View: return current config.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
