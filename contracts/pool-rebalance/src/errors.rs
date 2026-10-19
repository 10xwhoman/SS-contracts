use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    /// Weights provided do not sum to 10 000 bps.
    InvalidWeights = 4,
    /// The proposal does not exist.
    ProposalNotFound = 5,
    /// execute_rebalance called before the 24h timelock has elapsed.
    TimeNotReached = 6,
    /// The proposal has already been executed.
    AlreadyExecuted = 7,
    /// The pool referenced in the proposal does not exist.
    PoolNotFound = 8,
    InvalidAmount = 9,
    Overflow = 10,
}
