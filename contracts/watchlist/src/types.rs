use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Operator,
    Watchlist(Address),
    InvoiceWatchers(BytesN<32>),
}
