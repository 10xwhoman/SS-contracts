//! Error types for the referral contract.
use soroban_sdk::contracterror;

/// Errors that can occur during referral contract operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized (e.g. not admin or not the bound escrow).
    Unauthorized = 3,
    /// Referee already has a referrer registered.
    AlreadyReferred = 4,
    /// Referee and referrer are the same address.
    SelfReferral = 5,
    /// No referrer found for the given referee.
    NoReferrer = 6,
    /// No pending rewards to claim.
    NothingToClaim = 7,
    /// Insufficient contract balance to pay out rewards.
    InsufficientBalance = 8,
    /// Arithmetic overflow.
    Overflow = 9,
    /// Fee basis points exceed 10000 (100%).
    InvalidFeeBps = 10,
    /// Amount is zero or negative.
    InvalidAmount = 11,
}
