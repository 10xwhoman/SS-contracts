use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvoiceNotFound = 4,
    InvoiceNotSettled = 5,
    NotFractionHolder = 6,
    InvalidScore = 7,
    AlreadyRated = 8,
    InvoiceAlreadyRegistered = 9,
}
