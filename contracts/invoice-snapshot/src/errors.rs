use soroban_sdk::contracterror;

/// Errors that can occur during snapshot handling.
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
    /// Balance amount is negative.
    InvalidAmount = 4,
    /// Per-invoice holder cap reached.
    TooManyHolders = 5,
    /// No (live) snapshot exists for the given id.
    SnapshotNotFound = 6,
    /// Arithmetic overflow.
    Overflow = 7,
    /// The snapshot holds no balances to distribute against.
    NothingToDistribute = 8,
}
