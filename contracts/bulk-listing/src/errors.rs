use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InvalidPrice = 4,
    InvalidFee = 5,
    EmptyBatch = 6,
    BatchTooLarge = 7,
    BulkNotFound = 8,
    Overflow = 9,
}
