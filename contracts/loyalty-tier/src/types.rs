use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Tier {
    Bronze = 0,
    Silver = 1,
    Gold = 2,
    Platinum = 3,
}

/// Minimum cumulative volume required to enter each tier above Bronze.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoyaltyThresholds {
    pub silver: i128,
    pub gold: i128,
    pub platinum: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoyaltyInfo {
    pub tier: Tier,
    pub cumulative_volume: i128,
    /// Volume at which the next tier starts, `None` for the top tier.
    pub next_threshold: Option<i128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Escrow,
    Thresholds,
    Volume(Address),
}
