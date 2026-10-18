//! Error types for the portfolio snapshot contract.

use soroban_sdk::contracterror;

/// Errors that can occur during contract execution.
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
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 4,
    /// No snapshot exists for the given wallet and snapshot id.
    SnapshotNotFound = 5,
    /// The token registry has reached its configured capacity.
    TooManyTokens = 6,
    /// No registered token exists for the given invoice id.
    TokenNotFound = 7,
}
