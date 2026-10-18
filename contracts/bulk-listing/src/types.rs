use soroban_sdk::{contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkItem {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub price: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Listing {
    pub listing_id: u32,
    pub bulk_id: u32,
    pub seller: Address,
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub price: i128,
    pub fee: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkRecord {
    pub seller: Address,
    pub listing_ids: Vec<u32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Token,
    FeeBps,
    FeeRecipient,
    NextListingId,
    NextBulkId,
    Listing(u32),
    Bulk(u32),
}
