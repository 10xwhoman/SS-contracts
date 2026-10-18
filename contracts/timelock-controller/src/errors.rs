//! Error types for the timelock controller contract.
use soroban_sdk::contracterror;

/// Errors that can occur during timelock operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not the authorized admin.
    Unauthorized = 3,
    /// eta does not satisfy the minimum delay (too soon).
    DelayTooShort = 4,
    /// Action cannot be executed before eta.
    NotYetExecutable = 5,
    /// No action found for the given id.
    ActionNotFound = 6,
    /// Action has already been executed.
    AlreadyExecuted = 7,
    /// Action has been cancelled.
    ActionCancelled = 8,
    /// Minimum delay is invalid (zero).
    InvalidDelay = 9,
    /// eta is zero.
    InvalidEta = 10,
}
