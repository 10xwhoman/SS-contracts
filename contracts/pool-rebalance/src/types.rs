use soroban_sdk::{contracttype, Address, Vec};

/// State for a pool being tracked by this contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolState {
    /// Total value locked in the pool (sum of all investor positions).
    pub tvl: i128,
    /// Current allocation weights (bps, sum = 10000).
    pub weights: Vec<u32>,
    /// Number of distinct investors tracked.
    pub investor_count: u32,
}

/// A pending rebalance proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebalanceProposal {
    pub proposal_id: u32,
    pub pool_id: u32,
    /// Proposed new weights (bps, must sum to 10000).
    pub new_weights: Vec<u32>,
    /// Ledger timestamp at which the proposal was created.
    pub proposed_at: u64,
    /// Earliest time at which execute_rebalance may be called.
    pub executable_after: u64,
    pub executed: bool,
}

/// Per-investor position in a pool.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorPosition {
    /// Raw amount the investor has deposited.
    pub deposited: i128,
    /// Pro-rata entitlement to each invoice, computed from weights.
    /// Index-aligned with the pool's weight vector.
    pub allocations: Vec<i128>,
}

/// Impact of a proposed rebalance on a single investor.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorImpact {
    pub investor: Address,
    pub old_allocations: Vec<i128>,
    pub new_allocations: Vec<i128>,
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    /// Minimum delay between proposal and execution (seconds).
    pub timelock_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Config,
    NextProposalId,
    Proposal(u32),
    Pool(u32),
    /// (pool_id, investor) → InvestorPosition
    Position(u32, Address),
    /// pool_id → Vec<Address> of investors
    Investors(u32),
}

pub const BPS_DENOMINATOR: u32 = 10_000;
/// Default rebalance timelock: 24 hours.
pub const DEFAULT_TIMELOCK: u64 = 24 * 3_600;
