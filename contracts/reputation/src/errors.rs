//! Error types for the reputation contract.
use soroban_sdk::contracterror;

/// Errors that can occur during reputation contract operations.
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
    /// Arithmetic overflow.
    Overflow = 4,
}
