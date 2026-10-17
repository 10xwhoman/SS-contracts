use soroban_sdk::{contracttype, Address, BytesN, String};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Sector {
    Manufacturing = 0,
    Retail = 1,
    Logistics = 2,
    Healthcare = 3,
    Technology = 4,
    Other = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryInfo {
    pub invoice_id: BytesN<32>,
    pub issuer: Address,
    pub sector: Sector,
    pub sub_category: String,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Category(BytesN<32>),
    SectorInvoices(Sector),
}
