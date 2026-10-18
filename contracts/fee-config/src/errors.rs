//! Error types for the fee configuration contract.
use soroban_sdk::contracterror;

/// Errors that can occur during fee configuration.
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
    /// Fee rate is outside the configured [min_bps, max_bps] bounds.
    FeeOutOfBounds = 4,
    /// Minimum bound is greater than maximum bound.
    InvalidBounds = 5,
    /// Bounds exceed the 10 000 bps (100%) hard ceiling.
    BoundsExceedMax = 6,
    /// Unknown / unsupported fee type.
    InvalidFeeType = 7,
}
