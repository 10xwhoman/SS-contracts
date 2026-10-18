//! Data types for the multi-sig admin contract.

use soroban_sdk::{contracttype, Address, Symbol, Vec};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: signer set, threshold, pause flag, and current fee.
    Config,
    /// Persistent: proposal by id.
    Proposal(u32),
    /// Persistent: next proposal id to assign.
    NextProposalId,
    /// Persistent: whitelist flag by account.
    Whitelist(Address),
    /// Persistent: settlement-trigger flag by invoice id.
    SettlementTriggered(Symbol),
}

/// The privileged platform operations gated behind the multi-sig flow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ActionType {
    /// Add or remove an account from the platform whitelist.
    WhitelistUpdate = 0,
    /// Change the platform fee in basis points.
    FeeChange = 1,
    /// Trigger settlement for an invoice.
    SettlementTrigger = 2,
    /// Pause privileged operations platform-wide.
    Pause = 3,
    /// Resume privileged operations platform-wide.
    Resume = 4,
    /// Replace the signer set and threshold (requires current threshold).
    UpdateSigners = 5,
}

/// Lifecycle status of a proposal.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ProposalStatus {
    /// Collecting signatures.
    Pending = 0,
    /// Threshold reached and action applied.
    Executed = 1,
    /// Rejected by a signer; permanently blocks execution.
    Rejected = 2,
}

/// Generic parameter bag for an action. Only the fields relevant to the
/// proposal's `ActionType` are read; the rest are ignored at execution time.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionParams {
    /// Whitelist target account (`WhitelistUpdate`).
    pub account: Option<Address>,
    /// Invoice id (`SettlementTrigger`).
    pub invoice_id: Option<Symbol>,
    /// Fee in basis points (`FeeChange`) or any numeric payload.
    pub amount: Option<i128>,
    /// Whitelist add (true) / remove (false) flag (`WhitelistUpdate`).
    pub allowed: Option<bool>,
    /// Replacement signer set (`UpdateSigners`).
    pub new_signers: Option<Vec<Address>>,
    /// Replacement threshold (`UpdateSigners`).
    pub new_threshold: Option<u32>,
}

/// A proposal awaiting signatures/execution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    /// Sequential proposal id (starts at 1).
    pub id: u32,
    /// Signer that created the proposal. Counts as the first approval.
    pub proposer: Address,
    /// The action to perform when executed.
    pub action_type: ActionType,
    /// Parameters for the action.
    pub params: ActionParams,
    /// Signers that have approved (includes the proposer).
    pub approvals: Vec<Address>,
    /// Signer that rejected the proposal, if any.
    pub rejected_by: Option<Address>,
    /// Current status.
    pub status: ProposalStatus,
    /// Ledger timestamp of creation.
    pub created_at: u64,
    /// Ledger timestamp of execution (0 while pending/rejected).
    pub executed_at: u64,
}

/// Global multi-sig configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Authorized signer set.
    pub signers: Vec<Address>,
    /// Number of approvals required to execute a proposal.
    pub threshold: u32,
    /// Whether privileged operations are paused.
    pub paused: bool,
    /// Current platform fee in basis points.
    pub fee_bps: u32,
}
