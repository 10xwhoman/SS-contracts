//! Fee configuration contract for StellarSettle.
//!
//! Stores all platform fee rates — investment fee, secondary market fee,
//! settlement fee — with admin update controls and on-chain change history.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

use errors::Error;
use storage::MAX_HISTORY;
use types::{Config, FeeHistoryEntry, FeeSchedule, FeeType};

/// Hard ceiling: 100% expressed in basis points.
const HARD_MAX_BPS: u32 = 10_000;

#[contract]
pub struct FeeConfig;

#[contractimpl]
impl FeeConfig {
    /// Initialize the contract with admin and fee bounds (basis points).
    pub fn initialize(
        env: Env,
        admin: Address,
        min_bps: u32,
        max_bps: u32,
    ) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        if min_bps > max_bps {
            return Err(Error::InvalidBounds);
        }
        if max_bps > HARD_MAX_BPS {
            return Err(Error::BoundsExceedMax);
        }
        let config = Config {
            admin,
            min_bps,
            max_bps,
        };
        storage::set_config(&env, &config);
        storage::set_initialized(&env);
        Ok(())
    }

    /// Admin-only: set the fee rate for a fee type. Validates the rate is
    /// within the configured [min_bps, max_bps] bounds before storing.
    pub fn set_fee(env: Env, admin: Address, fee_type: FeeType, rate_bps: u32) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        admin.require_auth();
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if rate_bps < config.min_bps || rate_bps > config.max_bps {
            return Err(Error::FeeOutOfBounds);
        }
        let old_bps = storage::get_fee_rate(&env, fee_type);
        storage::set_fee_rate(&env, fee_type, rate_bps);

        // Record in the audit history (newest first, capped at MAX_HISTORY).
        let now = env.ledger().timestamp();
        let entry = FeeHistoryEntry {
            old_bps,
            new_bps: rate_bps,
            admin: config.admin.clone(),
            timestamp: now,
        };
        let mut history = storage::get_fee_history(&env, fee_type);
        history.push_front(entry);
        if history.len() > MAX_HISTORY {
            history.pop_back();
        }
        storage::set_fee_history(&env, fee_type, &history);

        events::fee_updated(&env, fee_type, old_bps, rate_bps, &config.admin, now);
        Ok(())
    }

    /// View: returns the current rate (bps) for a fee type.
    pub fn get_fee(env: Env, fee_type: FeeType) -> Result<u32, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_fee_rate(&env, fee_type))
    }

    /// View: returns the complete fee schedule (all three fee types).
    pub fn get_all_fees(env: Env) -> Result<FeeSchedule, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(FeeSchedule {
            investment: storage::get_fee_rate(&env, FeeType::Investment),
            secondary: storage::get_fee_rate(&env, FeeType::Secondary),
            settlement: storage::get_fee_rate(&env, FeeType::Settlement),
        })
    }

    /// View: returns the last 50 change records for a fee type, newest first.
    pub fn get_fee_history(env: Env, fee_type: FeeType) -> Result<Vec<FeeHistoryEntry>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_fee_history(&env, fee_type))
    }

    /// Admin-only: update the fee bounds. Existing rates that fall outside the
    /// new bounds are **not** retroactively clamped — subsequent `set_fee`
    /// calls will enforce the new range.
    pub fn set_bounds(env: Env, admin: Address, min_bps: u32, max_bps: u32) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        admin.require_auth();
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if min_bps > max_bps {
            return Err(Error::InvalidBounds);
        }
        if max_bps > HARD_MAX_BPS {
            return Err(Error::BoundsExceedMax);
        }
        let old_min = config.min_bps;
        let old_max = config.max_bps;
        let new_config = Config {
            admin: config.admin.clone(),
            min_bps,
            max_bps,
        };
        storage::set_config(&env, &new_config);
        events::bounds_updated(&env, old_min, old_max, min_bps, max_bps);
        Ok(())
    }

    /// View: returns the current bounds.
    pub fn get_bounds(env: Env) -> Result<(u32, u32), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok((config.min_bps, config.max_bps))
    }
}
