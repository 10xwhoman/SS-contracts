use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoInvestCriteria {
    pub min_yield_bps: u32,
    pub max_duration_days: u32,
    pub max_risk_tier: u32,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoInvestRule {
    pub wallet: Address,
    pub criteria: AutoInvestCriteria,
    pub max_amount_per_invoice: i128,
    pub total_budget: i128,
    pub total_invested: i128,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceListing {
    pub invoice_id: BytesN<32>,
    pub yield_bps: u32,
    pub duration_days: u32,
    pub risk_tier: u32,
    pub token: Address,
    pub funding_needed: i128,
    pub is_open: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Operator,
    Rule(Address),
    Listing(BytesN<32>),
}
