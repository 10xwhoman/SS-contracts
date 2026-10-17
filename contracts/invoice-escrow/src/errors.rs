//! Error types for the invoice escrow contract.
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
    /// Amount is zero or negative.
    InvalidAmount = 4,
    /// Platform fee basis points exceed 10000 (100%).
    InvalidFeeBps = 5,
    /// No escrow exists for the given invoice.
    EscrowNotFound = 6,
    /// Escrow already exists for this invoice (duplicate create).
    EscrowExists = 7,
    /// Escrow has already been funded.
    EscrowFunded = 8,
    /// Escrow has not been funded yet.
    EscrowNotFunded = 9,
    /// Payment has already been settled or escrow refunded.
    AlreadySettled = 10,
    /// Refund not allowed (e.g. not past due date or wrong status).
    RefundNotAllowed = 11,
    /// Token transfer failed (e.g. insufficient balance).
    TransferFailed = 12,
    /// Arithmetic overflow or invalid operation.
    Overflow = 13,
    /// Escrow has been cancelled by the seller.
    EscrowCancelled = 14,
    /// Contract is paused and the requested operation is temporarily disabled.
    Paused = 15,
    /// Payer is not the authorized debtor for this invoice.
    InvalidPayer = 16,
    /// Due date is invalid (e.g., in the past or zero).
    InvalidDueDate = 17,
    /// Asset decimals for payment token and invoice token do not align.
    InvalidAssetDecimals = 18,
    /// Nonce has already been consumed by a prior signed off-chain approval (replay attempt).
    NonceAlreadyUsed = 19,
    /// Escrow is not yet in a terminal state (Settled, Refunded, or Cancelled) and cannot be cleaned up.
    EscrowNotSettled = 20,
    /// Buyer is not whitelisted to fund escrows.
    NotWhitelisted = 21,
    /// Off-chain signature has expired (timestamp too old).
    SignatureExpired = 22,
    /// Funding amount does not meet the required milestone threshold.
    InvalidMilestoneAmount = 23,
    /// Cannot cancel because escrow is not in the correct state.
    CancelNotAllowed = 24,
    /// Penalty configuration is invalid (e.g. rate exceeds maximum).
    InvalidPenaltyConfig = 25,
    /// Payment token contract is invalid or does not implement the token interface.
    InvalidPaymentToken = 26,
    /// Invoice token contract is invalid or does not implement the required interface.
    InvalidInvoiceToken = 27,
    /// Payment token and invoice token must be different contracts.
    IdenticalTokens = 28,
    /// Investment amount is below the per-invoice minimum investment floor.
    /// A minimum of 0 means no floor; this error is only returned when the
    /// configured minimum is positive and the attempted amount is strictly less.
    MinimumInvestmentViolation = 29,
}
