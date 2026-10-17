use soroban_sdk::contracterror;

/// Errors that can occur during maturity enforcement.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not authorized (e.g. not the configured admin).
    Unauthorized = 3,
    /// Maturity has already been configured for this invoice (set-once).
    MaturityAlreadySet = 4,
    /// No maturity exists for the given invoice.
    MaturityNotFound = 5,
    /// Maturity timestamp must be strictly in the future.
    InvalidMaturity = 6,
    /// `trigger_maturity` was called before the maturity timestamp.
    NotMature = 7,
    /// Settlement was already triggered for this invoice.
    SettlementAlreadyTriggered = 8,
}
