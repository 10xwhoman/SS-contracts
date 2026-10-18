//! Data types for the fee configuration contract.

use soroban_sdk::{contracttype, Address};

/// Platform fee categories tracked by this contract.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeType {
    /// Fee charged on primary investment (funding) flows.
    Investment = 0,
    /// Fee charged on secondary-market trades.
    Secondary = 1,
    /// Fee charged on settlement / payment distribution.
    Settlement = 2,
}

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config (admin + bounds).
    Config,
    /// Instance: current rate for a fee type.
    FeeRate(FeeType),
    /// Persistent: rolling change history for a fee type (newest first).
    FeeHistory(FeeType),
    /// Instance: whether the contract has been initialized.
    Initialized,
}

/// Global configuration set at initialization.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin authorized to update fees and bounds.
    pub admin: Address,
    /// Minimum allowed fee rate (basis points).
    pub min_bps: u32,
    /// Maximum allowed fee rate (basis points).
    pub max_bps: u32,
}

/// One entry in the fee change audit history.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeHistoryEntry {
    /// Fee rate before this change.
    pub old_bps: u32,
    /// Fee rate after this change.
    pub new_bps: u32,
    /// Admin that performed the update.
    pub admin: Address,
    /// Ledger timestamp of the change.
    pub timestamp: u64,
}

/// Complete fee schedule returned by `get_all_fees`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeSchedule {
    /// Investment fee rate (bps).
    pub investment: u32,
    /// Secondary market fee rate (bps).
    pub secondary: u32,
    /// Settlement fee rate (bps).
    pub settlement: u32,
}
