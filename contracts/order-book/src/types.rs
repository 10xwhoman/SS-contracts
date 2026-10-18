use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuyOrder {
    pub order_id: u32,
    pub buyer: Address,
    pub invoice_id: BytesN<32>,
    pub fraction_amount: i128,
    pub max_price_per_fraction: i128,
    pub escrowed_funds: i128,
    pub is_active: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchResult {
    pub order_id: u32,
    pub buyer: Address,
    pub seller: Address,
    pub matched_amount: i128,
    pub execution_price: i128,
    pub total_cost: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Token,
    NextOrderId,
    Order(u32),
    InvoiceOrders(BytesN<32>),
}
