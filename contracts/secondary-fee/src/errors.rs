//! Error types for the secondary market fee contract.

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
    /// Caller is not authorized (e.g. not admin or not the secondary market).
    Unauthorized = 3,
    /// Sale amount is zero or negative.
    InvalidAmount = 4,
    /// Fee basis points exceed 10000 (100%).
    InvalidFeeBps = 5,
    /// No secondary market contract has been bound yet.
    SecondaryMarketNotSet = 6,
    /// The fee contract (or the caller) cannot cover the sale amount.
    InsufficientBalance = 7,
    /// Address is invalid (zero address, or the contract itself).
    InvalidAddress = 8,
    /// Arithmetic overflow.
    Overflow = 9,
    /// No invoice registry has been configured.
    RegistryNotSet = 10,
    /// The invoice is not classified in the invoice registry.
    InvoiceNotRegistered = 11,
}
