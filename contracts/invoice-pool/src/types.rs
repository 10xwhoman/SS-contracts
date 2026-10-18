use soroban_sdk::{contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pool {
    pub invoice_ids: Vec<BytesN<32>>,
    /// Allocation weights in bps, summing to 10000.
    pub weights: Vec<u32>,
    /// Amount invested into each invoice, index-aligned with `invoice_ids`.
    pub allocations: Vec<i128>,
    pub tvl: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolComposition {
    pub invoice_ids: Vec<BytesN<32>>,
    pub weights: Vec<u32>,
    pub allocations: Vec<i128>,
    pub tvl: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Token,
    NextPoolId,
    Pool(u32),
    InvoiceYield(BytesN<32>),
    Position(u32, Address),
}
