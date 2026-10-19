//! Error types for the fraud report registry contract.
use soroban_sdk::contracterror;

/// Errors that can occur during fraud report operations.
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
    /// Caller is not a whitelisted investor.
    NotWhitelisted = 4,
    /// This wallet already has an active report against the invoice.
    AlreadyReported = 5,
    /// No report found for the given report id.
    ReportNotFound = 6,
    /// Report has already been resolved.
    AlreadyResolved = 7,
    /// Report is not in an active state.
    NotActive = 8,
    /// Already whitelisted.
    AlreadyWhitelisted = 9,
    /// Not currently whitelisted (cannot remove).
    NotWhitelistedErr = 10,
    /// No invoice registry has been configured.
    RegistryNotSet = 11,
}
