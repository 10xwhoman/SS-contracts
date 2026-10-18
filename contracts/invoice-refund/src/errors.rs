use soroban_sdk::contracterror;

/// Errors that can occur during refund processing.
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
    /// Amount is zero or negative.
    InvalidAmount = 4,
    /// An invoice with the given id has already been registered.
    InvoiceAlreadyExists = 5,
    /// No invoice exists for the given id.
    InvoiceNotFound = 6,
    /// Expiry timestamp must be strictly in the future at registration time.
    InvalidExpiry = 7,
    /// `trigger_refund` was called before the invoice expired.
    InvoiceNotExpired = 8,
    /// The invoice reached its funding threshold and is not refund-eligible.
    InvoiceFunded = 9,
    /// Invoice has already been refunded (re-trigger rejected).
    AlreadyRefunded = 10,
    /// Per-invoice investor cap reached.
    TooManyInvestors = 11,
    /// Arithmetic overflow.
    Overflow = 12,
    /// Invoice has expired; new investments are no longer accepted.
    InvoiceExpired = 13,
    /// Investment amount exceeds the remaining capacity toward the funding threshold.
    ExceedsFundingThreshold = 14,
}
