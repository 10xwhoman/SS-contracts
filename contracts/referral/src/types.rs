//! Data types for the referral contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config.
    Config,
    /// Persistent: referrer address for a given referee (one-time binding).
    Referrer(soroban_sdk::Address),
    /// Persistent: pending (unclaimed) rewards balance for a referrer.
    Pending(soroban_sdk::Address),
    /// Persistent: total lifetime rewards earned by a referrer (including claimed).
    TotalEarned(soroban_sdk::Address),
    /// Persistent: number of referees registered under a referrer.
    RefCount(soroban_sdk::Address),
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address (can update fee rate and escrow binding).
    pub admin: soroban_sdk::Address,
    /// Referral fee in basis points (e.g. 500 = 5% of investment amount).
    pub fee_bps: u32,
    /// Payment token contract used for reward payouts.
    pub token: soroban_sdk::Address,
    /// Optional escrow contract address allowed to call accrue_referral_fee.
    /// When set, only that escrow may accrue fees.
    pub escrow: Option<soroban_sdk::Address>,
}

/// Aggregated referral statistics for a referrer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralStats {
    /// Total lifetime rewards earned (including already-claimed).
    pub total_earned: i128,
    /// Current pending (unclaimed) rewards.
    pub pending: i128,
    /// Number of referees registered under this referrer.
    pub referee_count: u32,
}
