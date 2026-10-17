//! Error types for the waitlist contract.
use soroban_sdk::contracterror;

/// Errors that can occur during waitlist contract operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized (e.g. not admin).
    Unauthorized = 3,
    /// Wallet is already on the waitlist.
    AlreadyOnWaitlist = 4,
    /// Wallet is not on the waitlist.
    NotOnWaitlist = 5,
    /// Wallet has already been approved.
    AlreadyApproved = 6,
    /// Invalid count for bulk approve (zero or exceeds queue size).
    InvalidCount = 7,
    /// Arithmetic overflow.
    Overflow = 8,
    /// Cross-contract call to the KYC whitelist failed.
    WhitelistCallFailed = 9,
}
