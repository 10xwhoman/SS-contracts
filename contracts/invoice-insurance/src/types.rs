use soroban_sdk::{contracttype, Address, BytesN};

pub const BPS_DENOMINATOR: i128 = 10_000;
/// Premium rate: 2% of the coverage amount.
pub const PREMIUM_BPS: i128 = 200;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub investor: Address,
    pub principal: i128,
    pub coverage_bps: u32,
    /// Maximum payout: `principal * coverage_bps / 10_000`.
    pub coverage_amount: i128,
    pub premium: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Token,
    Policy(BytesN<32>, Address),
    Policyholders(BytesN<32>),
    PayoutTriggered(BytesN<32>),
}
