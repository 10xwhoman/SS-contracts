use soroban_sdk::{contracttype, Address, Vec};

/// State of a syndicate.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SyndicateStatus {
    Open = 0,
    Executed = 1,
    Cancelled = 2,
}

/// A co-investment entry from a single co-investor.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoInvestment {
    pub investor: Address,
    pub amount: i128,
}

/// Full syndicate record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Syndicate {
    pub syndicate_id: u32,
    pub invoice_id: soroban_sdk::Symbol,
    pub lead_investor: Address,
    /// Minimum amount any single co-investor must commit.
    pub min_co_invest: i128,
    /// Target total amount required to execute.
    pub target_amount: i128,
    /// Timestamp after which uncommitted funds are returned.
    pub funding_deadline: u64,
    /// Committed co-investments (excluding lead's own commitment).
    pub co_investments: Vec<CoInvestment>,
    /// Total amount raised so far (including lead).
    pub total_raised: i128,
    pub status: SyndicateStatus,
}

/// Contract-wide config.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Config,
    NextId,
    Syndicate(u32),
}
