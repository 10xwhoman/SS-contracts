use soroban_sdk::{contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HolderShare {
    pub holder: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterimPayment {
    pub payment_index: u32,
    pub amount: i128,
    pub timestamp: u64,
    pub remaining_balance: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceSettlementState {
    pub invoice_id: BytesN<32>,
    pub issuer: Address,
    pub token: Address,
    pub total_amount: i128,
    pub cumulative_paid: i128,
    pub remaining_balance: i128,
    pub is_settled: bool,
    pub holders: Vec<HolderShare>,
    pub payments: Vec<InterimPayment>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Invoice(BytesN<32>),
}
