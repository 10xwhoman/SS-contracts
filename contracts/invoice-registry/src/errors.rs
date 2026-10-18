//! Error types for the invoice registry contract.

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
    /// No classification exists for the given invoice.
    NotRegistered = 4,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 5,
    /// The category index has reached its configured capacity.
    CategoryIndexFull = 6,
}
