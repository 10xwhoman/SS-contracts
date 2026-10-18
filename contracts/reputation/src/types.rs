//! Data types for the reputation contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: admin address.
    Admin,
    /// Persistent: list of settlement records for an issuer.
    Settlements(soroban_sdk::Address),
}

/// A single settlement outcome record for an issuer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementRecord {
    /// Invoice identifier associated with this settlement.
    pub invoice_id: soroban_sdk::Symbol,
    /// Days early (positive) or late (negative) relative to the due date.
    /// Zero means settled exactly on time.
    pub days_offset: i32,
    /// Ledger timestamp when the record was created.
    pub timestamp: u64,
}

/// Component breakdown of the reputation score.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreComponents {
    /// On-time rate as a percentage (0–100). On-time = settled on or before due date.
    pub on_time_rate: u32,
    /// Average days late across all settlements (early settlements count as
    /// zero days). Zero means every settlement was on time or early.
    pub avg_days_variance: i32,
    /// Number of settlements recorded.
    pub settlement_count: u32,
}

/// Full reputation score for an issuer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationScore {
    /// Overall score 0–100.
    pub score: u32,
    /// Component breakdown.
    pub components: ScoreComponents,
}
