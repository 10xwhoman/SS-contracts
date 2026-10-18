//! Data types for the upgrade proxy contract.

use soroban_sdk::{contracttype, Address, Vec};

/// Storage key enum for instance storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Global config (admin, signers, threshold, delay, frozen).
    Config,
    /// Current logic (implementation) address.
    Logic,
    /// Pending upgrade proposal (only one at a time).
    Proposal,
}

/// Global configuration for the proxy.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyConfig {
    /// Primary admin (proposes and cancels upgrades, manages freeze).
    pub admin: Address,
    /// Multi-sig signer set that must approve upgrades.
    pub signers: Vec<Address>,
    /// Number of signer approvals required to execute an upgrade.
    pub threshold: u32,
    /// Minimum delay (seconds) between queue and execute.
    pub min_delay: u64,
    /// Emergency freeze flag — halts all forwarding.
    pub frozen: bool,
}

/// A pending upgrade proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeProposal {
    /// Proposed new logic address.
    pub new_logic: Address,
    /// Who proposed the upgrade.
    pub proposer: Address,
    /// Timestamp when queued.
    pub queued_at: u64,
    /// Earliest execution timestamp.
    pub eta: u64,
    /// Addresses that have approved.
    pub approvals: Vec<Address>,
    /// Whether the upgrade has been applied.
    pub executed: bool,
    /// Whether the proposal was cancelled.
    pub cancelled: bool,
}
