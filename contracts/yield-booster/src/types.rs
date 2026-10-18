use soroban_sdk::{contracttype, Address, BytesN};

/// Mirrors the loyalty-tier contract's `Tier` so its responses can be decoded.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Tier {
    Bronze = 0,
    Silver = 1,
    Gold = 2,
    Platinum = 3,
}

/// Mirrors the loyalty-tier contract's `LoyaltyInfo`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoyaltyInfo {
    pub tier: Tier,
    pub cumulative_volume: i128,
    pub next_threshold: Option<i128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Settlement,
    LoyaltyContract,
    Token,
    BoostPool,
    Boost(Address, BytesN<32>),
}
