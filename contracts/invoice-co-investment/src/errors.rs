use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    /// The amount is below the minimum co-investment threshold.
    BelowMinimum = 3,
    /// The syndicate does not exist.
    SyndicateNotFound = 4,
    /// Caller is not the lead investor.
    NotLeadInvestor = 5,
    /// The funding window has closed without meeting the target.
    FundingWindowExpired = 6,
    /// execute_syndicate called before the funding target was met.
    TargetNotMet = 7,
    /// Syndicate has already been executed.
    AlreadyExecuted = 8,
    /// Syndicate has already been cancelled / refunded.
    AlreadyCancelled = 9,
    InvalidAmount = 10,
    Overflow = 11,
}
