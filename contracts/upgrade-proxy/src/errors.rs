//! Error types for the upgrade proxy contract.
use soroban_sdk::contracterror;

/// Errors that can occur during proxy operations.
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
    /// Caller is not a configured signer.
    NotSigner = 4,
    /// An upgrade proposal is already pending.
    ProposalPending = 5,
    /// No upgrade proposal exists.
    NoProposal = 6,
    /// Approval threshold not yet met.
    ThresholdNotMet = 7,
    /// Timelock delay has not elapsed yet.
    TimelockNotElapsed = 8,
    /// Proposal has already been executed.
    AlreadyExecuted = 9,
    /// Proposal has been cancelled.
    ProposalCancelled = 10,
    /// Proxy is frozen — all forwarding is halted.
    Frozen = 11,
    /// No logic address is set.
    LogicNotSet = 12,
    /// Signer has already approved.
    AlreadyApproved = 13,
    /// Threshold cannot exceed signer count.
    InvalidThreshold = 14,
    /// Minimum delay must be > 0.
    InvalidDelay = 15,
    /// New logic address is the zero address.
    InvalidAddress = 16,
    /// New logic is identical to current logic.
    SameLogic = 17,
}
