//! Multi-sig Admin contract for StellarSettle (issue #524).
//!
//! Gates privileged platform operations — whitelist updates, fee changes,
//! settlement triggers, and pause/resume — behind an n-of-m signer approval
//! flow. Any authorized signer can propose an action; a proposal executes only
//! once the current threshold of signers has approved it. A single rejection
//! permanently blocks execution. Updating the signer set or threshold itself
//! goes through the same flow, so it requires approval from the current
//! signers.
//!
//! Executed actions are recorded as observable contract state (whitelist map,
//! fee bps, pause flag, settlement-trigger flags) and emitted as events so
//! downstream contracts and indexers can react.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

pub use types::{ActionParams, ActionType, Config, Proposal, ProposalStatus};

use errors::Error;

/// Upper bound on the signer set, bounding approval-count loops.
const MAX_SIGNERS: u32 = 20;
/// Maximum fee expressible in basis points (100%).
const MAX_FEE_BPS: i128 = 10_000;

#[contract]
pub struct MultisigAdmin;

#[contractimpl]
impl MultisigAdmin {
    /// Initialize with the founding signer set and threshold. The admin must be
    /// a member of the initial set; afterwards there is no privileged admin —
    /// every change goes through the n-of-m flow.
    pub fn initialize(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        Self::validate_signer_set(&signers, threshold)?;

        let mut admin_is_signer = false;
        for signer in signers.iter() {
            if signer == admin {
                admin_is_signer = true;
                break;
            }
        }
        if !admin_is_signer {
            return Err(Error::AdminNotSigner);
        }

        storage::set_config(
            &env,
            &Config {
                signers,
                threshold,
                paused: false,
                fee_bps: 0,
            },
        );
        Ok(())
    }

    /// Create a proposal for a privileged action. Callable by any authorized
    /// signer; the proposer counts as the first approval. Returns the new
    /// proposal id.
    pub fn propose_action(
        env: Env,
        proposer: Address,
        action_type: ActionType,
        params: ActionParams,
    ) -> Result<u32, Error> {
        proposer.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Self::ensure_signer(&config, &proposer)?;
        Self::validate_params(&action_type, &params)?;

        let proposal_id = storage::get_next_proposal_id(&env)
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        storage::set_next_proposal_id(&env, proposal_id);

        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            action_type,
            params,
            approvals,
            rejected_by: None,
            status: ProposalStatus::Pending,
            created_at: env.ledger().timestamp(),
            executed_at: 0,
        };
        storage::set_proposal(&env, proposal_id, &proposal);

        events::action_proposed(&env, proposal_id, &proposer, action_type);
        Ok(proposal_id)
    }

    /// Record a signer's approval of a pending proposal. Signers may also
    /// approve a rejected proposal for the record, but it can never execute.
    pub fn sign_proposal(env: Env, signer: Address, proposal_id: u32) -> Result<(), Error> {
        signer.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Self::ensure_signer(&config, &signer)?;

        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(Error::ProposalNotFound)?;
        if proposal.status == ProposalStatus::Executed {
            return Err(Error::ProposalNotPending);
        }
        for approval in proposal.approvals.iter() {
            if approval == signer {
                return Err(Error::AlreadySigned);
            }
        }

        proposal.approvals.push_back(signer.clone());
        storage::set_proposal(&env, proposal_id, &proposal);

        events::proposal_signed(&env, proposal_id, &signer, proposal.approvals.len());
        Ok(())
    }

    /// Execute a proposal once the current threshold of signers has approved
    /// it. Callable by any authorized signer. Rejected proposals can never
    /// execute, even after collecting further approvals.
    pub fn execute_proposal(env: Env, executor: Address, proposal_id: u32) -> Result<(), Error> {
        executor.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Self::ensure_signer(&config, &executor)?;

        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(Error::ProposalNotFound)?;

        // Rejection wins permanently.
        if proposal.rejected_by.is_some() || proposal.status == ProposalStatus::Rejected {
            return Err(Error::ProposalWasRejected);
        }
        if proposal.status != ProposalStatus::Pending {
            return Err(Error::ProposalNotPending);
        }

        // Count only approvals from signers still in the current set: stale
        // approvals from a removed signer must not satisfy the threshold.
        let approvals = Self::count_valid_approvals(&proposal, &config);
        if approvals < config.threshold {
            return Err(Error::ThresholdNotReached);
        }

        Self::apply_action(&env, &mut config, &proposal)?;

        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = env.ledger().timestamp();
        storage::set_proposal(&env, proposal_id, &proposal);
        storage::set_config(&env, &config);

        events::proposal_executed(&env, proposal_id, &executor, proposal.action_type);
        Ok(())
    }

    /// Reject a pending proposal. A single signer rejection marks the proposal
    /// rejected and permanently blocks execution.
    pub fn reject_proposal(env: Env, rejecter: Address, proposal_id: u32) -> Result<(), Error> {
        rejecter.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Self::ensure_signer(&config, &rejecter)?;

        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(Error::ProposalNotFound)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(Error::ProposalNotPending);
        }

        proposal.rejected_by = Some(rejecter.clone());
        proposal.status = ProposalStatus::Rejected;
        storage::set_proposal(&env, proposal_id, &proposal);

        events::proposal_rejected(&env, proposal_id, &rejecter);
        Ok(())
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    /// Return a proposal by id.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Result<Proposal, Error> {
        storage::get_proposal(&env, proposal_id).ok_or(Error::ProposalNotFound)
    }

    /// Total number of proposals ever created.
    pub fn proposal_count(env: Env) -> u32 {
        storage::get_next_proposal_id(&env)
    }

    /// Return the current config.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// Return the current signer set.
    pub fn get_signers(env: Env) -> Result<Vec<Address>, Error> {
        Ok(storage::get_config(&env).ok_or(Error::NotInit)?.signers)
    }

    /// Return the current threshold.
    pub fn get_threshold(env: Env) -> Result<u32, Error> {
        Ok(storage::get_config(&env).ok_or(Error::NotInit)?.threshold)
    }

    /// Whether an account is currently an authorized signer.
    pub fn is_signer(env: Env, account: Address) -> bool {
        match storage::get_config(&env) {
            Some(config) => Self::ensure_signer(&config, &account).is_ok(),
            None => false,
        }
    }

    /// Whether privileged operations are paused.
    pub fn is_paused(env: Env) -> bool {
        storage::get_config(&env).map(|c| c.paused).unwrap_or(false)
    }

    /// Current platform fee in basis points.
    pub fn get_fee_bps(env: Env) -> u32 {
        storage::get_config(&env).map(|c| c.fee_bps).unwrap_or(0)
    }

    /// Whether an account has been whitelisted through the multi-sig.
    pub fn is_whitelisted(env: Env, account: Address) -> bool {
        storage::is_whitelisted(&env, &account)
    }

    /// Whether settlement was triggered through the multi-sig for an invoice.
    pub fn is_settlement_triggered(env: Env, invoice_id: Symbol) -> bool {
        storage::is_settlement_triggered(&env, &invoice_id)
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    fn ensure_signer(config: &Config, account: &Address) -> Result<(), Error> {
        for signer in config.signers.iter() {
            if signer == *account {
                return Ok(());
            }
        }
        Err(Error::Unauthorized)
    }

    /// Count approvals that belong to signers in the current set.
    fn count_valid_approvals(proposal: &Proposal, config: &Config) -> u32 {
        let mut count: u32 = 0;
        for approval in proposal.approvals.iter() {
            for signer in config.signers.iter() {
                if approval == signer {
                    count += 1;
                    break;
                }
            }
        }
        count
    }

    fn validate_signer_set(signers: &Vec<Address>, threshold: u32) -> Result<(), Error> {
        if signers.is_empty() || signers.len() > MAX_SIGNERS {
            return Err(Error::InvalidSignerSet);
        }
        if threshold == 0 || threshold > signers.len() {
            return Err(Error::InvalidThreshold);
        }
        for i in 0..signers.len() {
            for j in (i + 1)..signers.len() {
                if signers.get(i).unwrap() == signers.get(j).unwrap() {
                    return Err(Error::InvalidSignerSet);
                }
            }
        }
        Ok(())
    }

    fn validate_params(action_type: &ActionType, params: &ActionParams) -> Result<(), Error> {
        match action_type {
            ActionType::WhitelistUpdate => {
                if params.account.is_none() || params.allowed.is_none() {
                    return Err(Error::InvalidActionParams);
                }
            }
            ActionType::FeeChange => match params.amount {
                Some(bps) if (0..=MAX_FEE_BPS).contains(&bps) => {}
                _ => return Err(Error::InvalidActionParams),
            },
            ActionType::SettlementTrigger => {
                if params.invoice_id.is_none() {
                    return Err(Error::InvalidActionParams);
                }
            }
            ActionType::Pause | ActionType::Resume => {}
            ActionType::UpdateSigners => {
                let signers = match params.new_signers.as_ref() {
                    Some(signers) => signers,
                    None => return Err(Error::InvalidActionParams),
                };
                let threshold = match params.new_threshold {
                    Some(threshold) => threshold,
                    None => return Err(Error::InvalidActionParams),
                };
                Self::validate_signer_set(signers, threshold)?;
            }
        }
        Ok(())
    }

    /// Apply an executed action to contract state. Parameters were validated at
    /// proposal time and are immutable, so lookups are expected to succeed.
    fn apply_action(env: &Env, config: &mut Config, proposal: &Proposal) -> Result<(), Error> {
        match proposal.action_type {
            ActionType::WhitelistUpdate => {
                let account = proposal
                    .params
                    .account
                    .clone()
                    .ok_or(Error::InvalidActionParams)?;
                let allowed = proposal.params.allowed.ok_or(Error::InvalidActionParams)?;
                storage::set_whitelisted(env, &account, allowed);
            }
            ActionType::FeeChange => {
                let bps = proposal.params.amount.ok_or(Error::InvalidActionParams)?;
                config.fee_bps = bps as u32;
            }
            ActionType::SettlementTrigger => {
                let invoice_id = proposal
                    .params
                    .invoice_id
                    .clone()
                    .ok_or(Error::InvalidActionParams)?;
                storage::set_settlement_triggered(env, &invoice_id);
            }
            ActionType::Pause => {
                config.paused = true;
            }
            ActionType::Resume => {
                config.paused = false;
            }
            ActionType::UpdateSigners => {
                let signers = proposal
                    .params
                    .new_signers
                    .clone()
                    .ok_or(Error::InvalidActionParams)?;
                let threshold = proposal
                    .params
                    .new_threshold
                    .ok_or(Error::InvalidActionParams)?;
                config.signers = signers;
                config.threshold = threshold;
                events::signers_updated(env, proposal.id, config.signers.len(), threshold);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
