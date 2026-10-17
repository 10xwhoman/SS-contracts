use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    VestNotFound = 4,
    InvalidTrancheIndex = 5,
    TimeNotReached = 6,
    AlreadyClaimed = 7,
    EmptyTranches = 8,
    InvalidAmount = 9,
    NoEligibleTranches = 10,
    InvalidTimestamp = 11,
}
