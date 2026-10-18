//! Timelock controller contract for StellarSettle.
//!
//! Wraps privileged admin operations with a mandatory delay, giving the
//! community time to observe and react before execution. Actions are queued
//! with an `eta` timestamp and can only be executed after that time.

#![no_std]
#![allow(clippy::too_many_arguments, deprecated)]

mod errors;
mod events;
mod storage;
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Val, Vec};

use errors::Error;
use types::{PendingAction, TimelockAction, TimelockConfig};

/// Default minimum delay: 48 hours in seconds.
const DEFAULT_MIN_DELAY: u64 = 48 * 3_600;

#[contract]
pub struct TimelockController;

#[contractimpl]
impl TimelockController {
    /// Initialize the contract with an admin and optional custom minimum
    /// delay (in seconds). Defaults to 48 hours when `min_delay` is 0.
    pub fn initialize(env: Env, admin: Address, min_delay: u64) -> Result<(), Error> {
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        let delay = if min_delay == 0 {
            DEFAULT_MIN_DELAY
        } else {
            min_delay
        };
        storage::set_config(&env, &TimelockConfig { admin, min_delay: delay });
        Ok(())
    }

    /// Queue an action for future execution. The `eta` must be at least
    /// `min_delay` seconds in the future.
    pub fn queue_action(
        env: Env,
        proposer: Address,
        action_type: Symbol,
        params: Vec<Val>,
        eta: u64,
    ) -> Result<u32, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        proposer.require_auth();
        if eta == 0 {
            return Err(Error::InvalidEta);
        }
        let now = env.ledger().timestamp();
        let min_eta = now
            .checked_add(config.min_delay)
            .ok_or(Error::DelayTooShort)?;
        if eta < min_eta {
            return Err(Error::DelayTooShort);
        }

        let action_id = storage::next_action_id(&env);
        let action = TimelockAction {
            action_id,
            act_type: action_type.clone(),
            params: params.clone(),
            eta,
            proposer: proposer.clone(),
            queued: now,
            executed: false,
            cancelled: false,
        };
        storage::set_action(&env, &action);
        events::action_queued(&env, action_id, &action_type, eta, &proposer);
        Ok(action_id)
    }

    /// Execute a previously queued action. Callable only after the eta timestamp.
    pub fn execute_action(env: Env, action_id: u32) -> Result<(), Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        let mut action = storage::get_action(&env, action_id).ok_or(Error::ActionNotFound)?;
        if action.cancelled {
            return Err(Error::ActionCancelled);
        }
        if action.executed {
            return Err(Error::AlreadyExecuted);
        }
        let now = env.ledger().timestamp();
        if now < action.eta {
            return Err(Error::NotYetExecutable);
        }
        action.executed = true;
        storage::set_action(&env, &action);
        events::action_executed(&env, action_id, &action.act_type, action.eta);
        Ok(())
    }

    /// Admin-only: cancel a queued action.
    pub fn cancel_action(env: Env, action_id: u32) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        let mut action = storage::get_action(&env, action_id).ok_or(Error::ActionNotFound)?;
        if action.executed {
            return Err(Error::AlreadyExecuted);
        }
        if action.cancelled {
            return Err(Error::ActionCancelled);
        }
        action.cancelled = true;
        storage::set_action(&env, &action);
        events::action_cancelled(&env, action_id, &action.act_type);
        Ok(())
    }

    /// Admin-only: update the minimum delay.
    pub fn set_min_delay(env: Env, new_delay: u64) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        if new_delay == 0 {
            return Err(Error::InvalidDelay);
        }
        storage::set_config(&env, &TimelockConfig { admin: config.admin, min_delay: new_delay });
        Ok(())
    }

    /// View: returns the current minimum delay.
    pub fn get_min_delay(env: Env) -> Result<u64, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.min_delay)
    }

    /// View: returns a single action by id.
    pub fn get_action(env: Env, action_id: u32) -> Result<TimelockAction, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        storage::get_action(&env, action_id).ok_or(Error::ActionNotFound)
    }

    /// View: returns all pending (not executed, not cancelled) actions.
    pub fn get_queued_actions(env: Env) -> Result<Vec<PendingAction>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        let count = storage::get_action_count(&env);
        let mut pending = Vec::new(&env);
        for id in 1..=count {
            if let Some(action) = storage::get_action(&env, id) {
                if !action.executed && !action.cancelled {
                    pending.push_back(PendingAction {
                        action_id: action.action_id,
                        act_type: action.act_type,
                        params: action.params,
                        eta: action.eta,
                        proposer: action.proposer,
                        queued: action.queued,
                    });
                }
            }
        }
        Ok(pending)
    }
}
