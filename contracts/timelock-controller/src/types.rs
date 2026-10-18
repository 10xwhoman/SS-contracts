//! Data types for the timelock controller contract.

use soroban_sdk::{contracttype, Address, Symbol, Val, Vec};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config (admin + min_delay).
    Config,
    /// Persistent: action record by action id.
    Action(u32),
    /// Instance: next action id counter.
    Count,
}

/// Global configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockConfig {
    /// Admin authorized to cancel actions and reconfigure the delay.
    pub admin: Address,
    /// Minimum delay (seconds) enforced between queue and execute.
    pub min_delay: u64,
}

/// A queued timelock action awaiting execution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockAction {
    /// Sequential action id.
    pub action_id: u32,
    /// Machine-readable action category.
    pub act_type: Symbol,
    /// Opaque parameters for the action.
    pub params: Vec<Val>,
    /// Earliest execution timestamp (ledger time).
    pub eta: u64,
    /// Address that queued the action.
    pub proposer: Address,
    /// Timestamp when the action was queued.
    pub queued: u64,
    /// Whether the action has been executed.
    pub executed: bool,
    /// Whether the action has been cancelled.
    pub cancelled: bool,
}

/// Summary of a pending action returned by `get_queued_actions`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAction {
    /// Sequential action id.
    pub action_id: u32,
    /// Action category.
    pub act_type: Symbol,
    /// Opaque parameters.
    pub params: Vec<Val>,
    /// Earliest execution timestamp.
    pub eta: u64,
    /// Who queued it.
    pub proposer: Address,
    /// When it was queued.
    pub queued: u64,
}
