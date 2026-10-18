use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    OrderNotFound = 4,
    OrderNotActive = 5,
    InvalidAmount = 6,
    InvalidPrice = 7,
    InsufficientEscrow = 8,
}
