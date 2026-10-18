use soroban_sdk::contracterror;

/// Errors that can occur in the multi-sig admin flow.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not an authorized signer.
    Unauthorized = 3,
    /// Threshold must be between 1 and the signer count.
    InvalidThreshold = 4,
    /// Signer set is empty, has duplicates, or exceeds the cap.
    InvalidSignerSet = 5,
    /// No proposal exists with the given id.
    ProposalNotFound = 6,
    /// Proposal is not pending (already executed).
    ProposalNotPending = 7,
    /// Signer already approved this proposal.
    AlreadySigned = 8,
    /// Not enough approvals collected to execute.
    ThresholdNotReached = 9,
    /// Proposal was rejected; execution is permanently blocked.
    ProposalWasRejected = 10,
    /// Action parameters are missing or invalid for the action type.
    InvalidActionParams = 11,
    /// Arithmetic overflow.
    Overflow = 12,
    /// Admin must be a member of the initial signer set.
    AdminNotSigner = 13,
}
