use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    EmptyBatch = 4,
    BatchLimitExceeded = 5,
    InvoiceNotFound = 6,
    InvoiceNotFunded = 7,
    InvoiceAlreadySettled = 8,
    InvoiceNotMatured = 9,
    InvalidAmount = 10,
    InvalidShares = 11,
}
