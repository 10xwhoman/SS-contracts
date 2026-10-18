use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidSplitRatios = 4,
    InvalidAmount = 5,
    NoBalanceToWithdraw = 6,
    IssuerNotFound = 7,
}
