use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRatios {
    pub protocol_bps: u32,
    pub issuer_bps: u32,
    pub lp_pool_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitAmounts {
    pub protocol_amount: i128,
    pub issuer_amount: i128,
    pub lp_pool_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    MarketOperator,
    Token,
    SplitRatios,
    ProtocolRecipient,
    LpPoolRecipient,
    InvoiceIssuer(BytesN<32>),
    Balance(Address),
    TotalAccumulated(Address),
}
