//! Upgrade proxy contract for StellarSettle.
//!
//! Stores a logic (implementation) address and forwards calls to it.
//! Logic upgrades are gated behind a timelock delay and multi-sig approval
//! threshold. An emergency freeze halts all forwarding during upgrade windows.
//!
//! NOTE: Soroban does not support EVM-style `delegatecall`. This contract
//! forwards calls to the logic via `invoke_contract`, so the logic contract
//! sees the proxy as the caller (not the original user). For true
//! state-preserving upgrades, consider `env.update_current_contract_wasm`.
//! This contract implements the governance layer (timelock + multi-sig +
//! freeze + events) and a delegation entry point as specified.

#![no_std]
#![allow(clippy::too_many_arguments, deprecated)]

mod errors;
mod events;
mod storage;
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Val, Vec};

use errors::Error;
use types::{ProxyConfig, UpgradeProposal};

#[contract]
pub struct UpgradeProxy;

/// Check whether `addr` is in `list`.
fn contains(list: &Vec<Address>, addr: &Address) -> bool {
    for entry in list.iter() {
        if entry == *addr {
            return true;
        }
    }
    false
}

#[contractimpl]
impl UpgradeProxy {
    /// Initialize the proxy with admin, multi-sig signers, threshold,
    /// minimum timelock delay, and the initial logic address.
    pub fn initialize(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
        min_delay: u64,
        initial_logic: Address,
    ) -> Result<(), Error> {
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        if signers.is_empty() || threshold == 0 || threshold > signers.len() {
            return Err(Error::InvalidThreshold);
        }
        if min_delay == 0 {
            return Err(Error::InvalidDelay);
        }
        let config = ProxyConfig {
            admin,
            signers,
            threshold,
            min_delay,
            frozen: false,
        };
        storage::set_config(&env, &config);
        storage::set_logic(&env, &initial_logic);
        Ok(())
    }

    // ── Upgrade governance ────────────────────────────────────────────

    /// Admin-only: queue an upgrade to a new logic address. The upgrade
    /// becomes executable after `min_delay` seconds AND once the multi-sig
    /// approval threshold is met.
    pub fn upgrade_logic(env: Env, new_address: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let current = storage::get_logic(&env).ok_or(Error::LogicNotSet)?;
        if new_address == current {
            return Err(Error::SameLogic);
        }
        if storage::get_proposal(&env).is_some() {
            return Err(Error::ProposalPending);
        }

        let now = env.ledger().timestamp();
        let eta = now
            .checked_add(config.min_delay)
            .ok_or(Error::InvalidDelay)?;

        let proposal = UpgradeProposal {
            new_logic: new_address.clone(),
            proposer: config.admin.clone(),
            queued_at: now,
            eta,
            approvals: Vec::new(&env),
            executed: false,
            cancelled: false,
        };
        storage::set_proposal(&env, &proposal);
        events::upgrade_proposed(&env, &new_address, eta, &config.admin);
        Ok(())
    }

    /// A configured signer approves the pending upgrade proposal.
    /// Duplicate approvals from the same signer are rejected.
    pub fn approve_upgrade(env: Env, signer: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        signer.require_auth();

        if !contains(&config.signers, &signer) {
            return Err(Error::NotSigner);
        }

        let mut proposal = storage::get_proposal(&env).ok_or(Error::NoProposal)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::ProposalCancelled);
        }
        if contains(&proposal.approvals, &signer) {
            return Err(Error::AlreadyApproved);
        }

        proposal.approvals.push_back(signer.clone());
        let count = proposal.approvals.len();
        storage::set_proposal(&env, &proposal);
        events::upgrade_approved(&env, &signer, count, config.threshold);
        Ok(())
    }

    /// Execute the pending upgrade. Requires the timelock to have elapsed
    /// AND the approval threshold to be met.
    pub fn execute_upgrade(env: Env) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let proposal = storage::get_proposal(&env).ok_or(Error::NoProposal)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::ProposalCancelled);
        }
        let now = env.ledger().timestamp();
        if now < proposal.eta {
            return Err(Error::TimelockNotElapsed);
        }
        if proposal.approvals.len() < config.threshold {
            return Err(Error::ThresholdNotMet);
        }

        let old_logic = storage::get_logic(&env).ok_or(Error::LogicNotSet)?;
        storage::set_logic(&env, &proposal.new_logic);
        storage::clear_proposal(&env);
        events::logic_upgraded(&env, &old_logic, &proposal.new_logic);
        Ok(())
    }

    /// Admin-only: cancel the pending upgrade proposal.
    pub fn cancel_upgrade(env: Env) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        let proposal = storage::get_proposal(&env).ok_or(Error::NoProposal)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        let new_logic = proposal.new_logic.clone();
        storage::clear_proposal(&env);
        events::upgrade_cancelled(&env, &new_logic);
        Ok(())
    }

    // ── Emergency freeze ──────────────────────────────────────────────

    /// Admin-only: freeze the proxy, halting all `forward` calls.
    pub fn emergency_freeze(env: Env) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        config.frozen = true;
        storage::set_config(&env, &config);
        events::proxy_frozen(&env, true);
        Ok(())
    }

    /// Admin-only: lift the emergency freeze.
    pub fn unfreeze(env: Env) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        config.frozen = false;
        storage::set_config(&env, &config);
        events::proxy_frozen(&env, false);
        Ok(())
    }

    // ── Views ─────────────────────────────────────────────────────────

    /// View: returns the current logic (implementation) address.
    pub fn get_logic_address(env: Env) -> Result<Address, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        storage::get_logic(&env).ok_or(Error::LogicNotSet)
    }

    /// View: returns the pending upgrade proposal, if any.
    pub fn get_pending_proposal(env: Env) -> Result<Option<UpgradeProposal>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_proposal(&env))
    }

    /// View: is the proxy currently frozen?
    pub fn is_frozen(env: Env) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.frozen)
    }

    /// View: is `addr` a configured signer?
    pub fn is_signer(env: Env, addr: Address) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(contains(&config.signers, &addr))
    }

    // ── Delegation ─────────────────────────────────────────────────────

    /// Forward a call to the current logic contract. Blocked when frozen.
    /// Returns the raw `Val` produced by the logic contract.
    pub fn forward(env: Env, fn_name: Symbol, args: Vec<Val>) -> Result<Val, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.frozen {
            return Err(Error::Frozen);
        }
        let logic = storage::get_logic(&env).ok_or(Error::LogicNotSet)?;
        Ok(env.invoke_contract::<Val>(&logic, &fn_name, args))
    }
}
