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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierThresholds {
    pub silver: u32,
    pub gold: u32,
    pub platinum: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorMetrics {
    pub investment_count: u32,
    pub default_exposure: u32,
    pub early_exit_count: u32,
    pub total_volume: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorScore {
    pub wallet: Address,
    pub score: u32,
    pub tier: Tier,
    pub metrics: InvestorMetrics,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    TierThresholds,
    Authorized(Address),
    Metrics(Address),
    Score(Address),
}
