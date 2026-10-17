use soroban_sdk::{contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorShare {
    pub investor: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceInfo {
    pub invoice_id: BytesN<32>,
    pub total_repayment: i128,
    pub maturity_timestamp: u64,
    pub is_funded: bool,
    pub is_settled: bool,
    pub token: Address,
    pub investors: Vec<InvestorShare>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerInvoiceSummary {
    pub invoice_id: BytesN<32>,
    pub total_distributed: i128,
    pub investor_count: u32,
    pub settled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Operator,
    Invoice(BytesN<32>),
    SettlementSummary(BytesN<32>),
}
