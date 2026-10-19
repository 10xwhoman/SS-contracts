//! Pool rebalance contract for StellarSettle (issue #624).
//!
//! Extends invoice pooling to support admin-initiated pool rebalances.
//! Updates constituent invoice weights while preserving existing investor
//! positions and pro-rata entitlements.
//!
//! Flow:
//!   1. Admin calls `propose_rebalance(pool_id, new_weights)`.
//!   2. After 24 h timelock, admin calls `execute_rebalance(proposal_id)`.
//!   3. All investor positions are recomputed to the new weights.
//!
//! Read helpers:
//!   * `get_rebalance_preview` — returns per-investor impact without writing.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{
    Config, InvestorImpact, InvestorPosition, PoolState, RebalanceProposal, StorageKey,
};

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use types::{BPS_DENOMINATOR, DEFAULT_TIMELOCK};

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn load_config(env: &Env) -> Result<Config, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .ok_or(Error::NotInitialized)
}

fn load_pool(env: &Env, pool_id: u32) -> Result<PoolState, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Pool(pool_id))
        .ok_or(Error::PoolNotFound)
}

fn save_pool(env: &Env, pool_id: u32, pool: &PoolState) {
    env.storage()
        .persistent()
        .set(&StorageKey::Pool(pool_id), pool);
}

fn load_proposal(env: &Env, proposal_id: u32) -> Result<RebalanceProposal, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Proposal(proposal_id))
        .ok_or(Error::ProposalNotFound)
}

fn save_proposal(env: &Env, proposal: &RebalanceProposal) {
    env.storage()
        .persistent()
        .set(&StorageKey::Proposal(proposal.proposal_id), proposal);
}

fn next_proposal_id(env: &Env) -> u32 {
    let id: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::NextProposalId)
        .unwrap_or(1u32);
    env.storage()
        .instance()
        .set(&StorageKey::NextProposalId, &(id + 1));
    id
}

fn load_position(env: &Env, pool_id: u32, investor: &Address) -> InvestorPosition {
    env.storage()
        .persistent()
        .get(&StorageKey::Position(pool_id, investor.clone()))
        .unwrap_or(InvestorPosition {
            deposited: 0,
            allocations: Vec::new(env),
        })
}

fn save_position(env: &Env, pool_id: u32, investor: &Address, pos: &InvestorPosition) {
    env.storage()
        .persistent()
        .set(&StorageKey::Position(pool_id, investor.clone()), pos);
}

fn load_investors(env: &Env, pool_id: u32) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::Investors(pool_id))
        .unwrap_or_else(|| Vec::new(env))
}

fn save_investors(env: &Env, pool_id: u32, investors: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&StorageKey::Investors(pool_id), investors);
}

// ---------------------------------------------------------------------------
// Validation helper
// ---------------------------------------------------------------------------

fn validate_weights(weights: &Vec<u32>) -> bool {
    if weights.is_empty() {
        return false;
    }
    let mut total: u32 = 0;
    for i in 0..weights.len() {
        let w = weights.get_unchecked(i);
        if w == 0 {
            return false;
        }
        total = match total.checked_add(w) {
            Some(v) => v,
            None => return false,
        };
    }
    total == BPS_DENOMINATOR
}

/// Compute per-invoice allocations for an investor given weights and deposited
/// amount. Remainder from rounding goes to the last invoice.
fn compute_allocations(
    env: &Env,
    deposited: i128,
    weights: &Vec<u32>,
) -> Result<Vec<i128>, Error> {
    let count = weights.len();
    let mut allocs: Vec<i128> = Vec::new(env);
    let mut distributed: i128 = 0;
    let last = count - 1;
    for i in 0..count {
        let share = if i == last {
            deposited
                .checked_sub(distributed)
                .ok_or(Error::Overflow)?
        } else {
            deposited
                .checked_mul(weights.get_unchecked(i) as i128)
                .ok_or(Error::Overflow)?
                / BPS_DENOMINATOR as i128
        };
        distributed = distributed.checked_add(share).ok_or(Error::Overflow)?;
        allocs.push_back(share);
    }
    Ok(allocs)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PoolRebalance;

#[contractimpl]
impl PoolRebalance {
    /// Initialise the contract.
    ///
    /// * `admin`             – only this address may propose rebalances.
    /// * `timelock_seconds`  – override the default 24h timelock (pass 0 for
    ///                          default).
    pub fn initialize(env: Env, admin: Address, timelock_seconds: u64) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        let delay = if timelock_seconds == 0 {
            DEFAULT_TIMELOCK
        } else {
            timelock_seconds
        };
        env.storage().instance().set(
            &StorageKey::Config,
            &Config {
                admin,
                timelock_seconds: delay,
            },
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Pool management
    // -----------------------------------------------------------------------

    /// Admin-only: register a pool with initial weights. Must be called before
    /// any investor positions are recorded for that pool.
    pub fn register_pool(env: Env, pool_id: u32, initial_weights: Vec<u32>) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        cfg.admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&StorageKey::Pool(pool_id))
        {
            // Already registered; ignore
            return Ok(());
        }

        if !validate_weights(&initial_weights) {
            return Err(Error::InvalidWeights);
        }

        save_pool(
            &env,
            pool_id,
            &PoolState {
                tvl: 0,
                weights: initial_weights,
                investor_count: 0,
            },
        );
        Ok(())
    }

    /// Record an investor position in a pool. Called by the pool contract or
    /// admin to keep position data in sync with this rebalance contract.
    pub fn record_position(
        env: Env,
        pool_id: u32,
        investor: Address,
        deposited: i128,
    ) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        cfg.admin.require_auth();

        if deposited <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut pool = load_pool(&env, pool_id)?;
        let allocations = compute_allocations(&env, deposited, &pool.weights)?;

        let old_pos = load_position(&env, pool_id, &investor);
        if old_pos.deposited == 0 {
            // New investor – register them
            let mut investors = load_investors(&env, pool_id);
            investors.push_back(investor.clone());
            save_investors(&env, pool_id, &investors);
            pool.investor_count = pool
                .investor_count
                .checked_add(1)
                .ok_or(Error::Overflow)?;
        }

        pool.tvl = pool
            .tvl
            .checked_add(deposited)
            .ok_or(Error::Overflow)?;
        save_pool(&env, pool_id, &pool);

        save_position(
            &env,
            pool_id,
            &investor,
            &InvestorPosition {
                deposited,
                allocations,
            },
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Rebalance lifecycle
    // -----------------------------------------------------------------------

    /// Admin-only: propose a new weight vector for a pool.
    ///
    /// `new_weights` must sum to 10 000 bps and have the same length as the
    /// pool's current weight vector. Returns the proposal id.
    pub fn propose_rebalance(
        env: Env,
        pool_id: u32,
        new_weights: Vec<u32>,
    ) -> Result<u32, Error> {
        let cfg = load_config(&env)?;
        cfg.admin.require_auth();

        let pool = load_pool(&env, pool_id)?;
        if new_weights.len() != pool.weights.len() {
            return Err(Error::InvalidWeights);
        }
        if !validate_weights(&new_weights) {
            return Err(Error::InvalidWeights);
        }

        let now = env.ledger().timestamp();
        let executable_after = now
            .checked_add(cfg.timelock_seconds)
            .ok_or(Error::Overflow)?;
        let proposal_id = next_proposal_id(&env);

        let proposal = RebalanceProposal {
            proposal_id,
            pool_id,
            new_weights: new_weights.clone(),
            proposed_at: now,
            executable_after,
            executed: false,
        };
        save_proposal(&env, &proposal);

        events::pool_rebalance_proposed(&env, proposal_id, pool_id, &new_weights, executable_after);
        Ok(proposal_id)
    }

    /// Admin-only: execute a rebalance proposal after the timelock expires.
    ///
    /// All existing investor positions are recomputed to the new weights.
    pub fn execute_rebalance(env: Env, proposal_id: u32) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        cfg.admin.require_auth();

        let mut proposal = load_proposal(&env, proposal_id)?;

        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }

        let now = env.ledger().timestamp();
        if now < proposal.executable_after {
            return Err(Error::TimeNotReached);
        }

        let mut pool = load_pool(&env, proposal.pool_id)?;
        pool.weights = proposal.new_weights.clone();
        save_pool(&env, proposal.pool_id, &pool);

        // Recompute all investor positions to the new weights.
        let investors = load_investors(&env, proposal.pool_id);
        for i in 0..investors.len() {
            let investor = investors.get_unchecked(i);
            let mut pos = load_position(&env, proposal.pool_id, &investor);
            let new_allocs = compute_allocations(&env, pos.deposited, &proposal.new_weights)?;
            pos.allocations = new_allocs;
            save_position(&env, proposal.pool_id, &investor, &pos);
        }

        proposal.executed = true;
        save_proposal(&env, &proposal);

        events::pool_rebalance_executed(&env, proposal_id, proposal.pool_id, &proposal.new_weights);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read helpers
    // -----------------------------------------------------------------------

    /// Returns the impact on every investor if the proposed weights were applied.
    ///
    /// Does **not** write to storage — purely a view / simulation call.
    pub fn get_rebalance_preview(
        env: Env,
        pool_id: u32,
        new_weights: Vec<u32>,
    ) -> Result<Vec<InvestorImpact>, Error> {
        load_config(&env)?;
        let pool = load_pool(&env, pool_id)?;

        if new_weights.len() != pool.weights.len() {
            return Err(Error::InvalidWeights);
        }
        if !validate_weights(&new_weights) {
            return Err(Error::InvalidWeights);
        }

        let investors = load_investors(&env, pool_id);
        let mut impacts: Vec<InvestorImpact> = Vec::new(&env);
        for i in 0..investors.len() {
            let investor = investors.get_unchecked(i);
            let pos = load_position(&env, pool_id, &investor);
            let new_allocs = compute_allocations(&env, pos.deposited, &new_weights)?;
            impacts.push_back(InvestorImpact {
                investor,
                old_allocations: pos.allocations,
                new_allocations: new_allocs,
            });
        }
        Ok(impacts)
    }

    /// Returns the pool state.
    pub fn get_pool(env: Env, pool_id: u32) -> Result<PoolState, Error> {
        load_config(&env)?;
        load_pool(&env, pool_id)
    }

    /// Returns a rebalance proposal.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Result<RebalanceProposal, Error> {
        load_config(&env)?;
        load_proposal(&env, proposal_id)
    }

    /// Returns an investor's current position.
    pub fn get_position(env: Env, pool_id: u32, investor: Address) -> Result<InvestorPosition, Error> {
        load_config(&env)?;
        Ok(load_position(&env, pool_id, &investor))
    }
}
