use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InvalidCoverage = 4,
    PolicyAlreadyExists = 5,
    PayoutAlreadyTriggered = 6,
    InsufficientPool = 7,
    Overflow = 8,
    InvalidLoss = 9,
}
