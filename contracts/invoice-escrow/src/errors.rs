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
    /// Investor has no position for this invoice.
    NoPositionFound = 29,
    /// Invoice status does not allow this operation.
    InvalidInvoiceStatus = 30,
    /// Remaining position after withdrawal is below the minimum investment floor.
    BelowMinimumInvestment = 31,
    /// Funding target has not yet been reached.
    FundingTargetNotReached = 32,
    /// Deposit amount is zero (dust prevention: use a positive amount).
    ZeroAmount = 33,
    /// Deposit amount is below the configured minimum investment.
    AmountBelowMinimum = 34,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 35,
    /// Escrow duration is outside the allowed [MIN, MAX] window.
    InvalidDuration = 36,
    /// Caller is not a member of the emergency admin multi-sig set.
    NotEmergencyAdmin = 37,
    /// Caller has already approved this emergency release (duplicate).
    AlreadyApproved = 38,
    /// Emergency release threshold has not been reached yet.
    ThresholdNotMet = 39,
    /// Emergency multi-sig config has not been set.
    EmergencyNotConfigured = 40,
    /// Pagination limit is invalid (zero).
    InvalidLimit = 41,
    /// Pagination limit exceeds maximum allowed page size.
    LimitExceeded = 42,
    /// Invoice with the given ID already exists.
    InvoiceAlreadyExists = 43,
    /// Yield basis points is invalid (must be between 1 and 5000).
    InvalidYield = 44,
    /// Funding deadline has not passed yet.
    FundingDeadlineNotPassed = 45,
    /// Repayment amount is less than the total raised amount.
    InsufficientRepayment = 46,
    /// New funding deadline must be greater than the current deadline.
    DeadlineNotExtended = 47,
    /// Payment or refund attempted after the due date (plus grace period) has passed.
    EscrowOverdue = 48,
    /// Refund attempted before the due date (plus grace period) has passed.
    EscrowNotOverdue = 49,
    /// Operation requires the escrow to be in the `Disputed` status.
    NotDisputed = 50,
    /// Dispute has already been resolved.
    AlreadyResolved = 51,
    /// Dispute resolution attempted after `dispute_timeout_secs` elapsed; only the
    /// default (refund-to-buyer) fallback is available past this point.
    DisputeTimedOut = 52,
    /// Early-settlement discount hook configuration is invalid, or the escrow is
    /// no longer in a state where configuring one is meaningful.
    InvalidEarlySettlement = 53,
    /// Installment repayment schedule is invalid (bad amounts/timestamps/sum),
    /// or cannot be modified in the escrow's current state.
    InvalidInstallmentSchedule = 54,
    /// Settlement fee exceeds the protocol's 10% safety ceiling.
    FeeTooHigh = 55,
    /// A registered invoice has reached its configured unique-investor cap.
    MaxInvestorsReached = 57,
    /// Investor cap is outside the supported range.
    InvalidMaxInvestors = 56,
    /// The requested payment token is outside the escrow's accepted token set.
    TokenNotAccepted = 58,
    /// The same administrator cannot propose and approve one settlement.
    SameAdminApproval = 59,
    /// No settlement proposal exists for the invoice.
    SettlementNotProposed = 60,
    /// Funding thresholds are invalid (must satisfy
    /// `0 < min <= funding_target <= max`).
    InvalidFundingThreshold = 61,
    /// Total raised is below the invoice's minimum funding threshold, so the
    /// proceeds cannot be released to the issuer yet.
    FundingThresholdNotReached = 62,
    /// Investment would push the total raised above the maximum funding
    /// threshold (invoice funding capacity is exhausted).
    MaxFundingThresholdExceeded = 63,
    /// Wallet's secondary-market buy limit for the current window was exceeded.
    BuyLimitExceeded = 64,
    /// The currency is not in the admin-configured supported currency list.
    UnsupportedCurrency = 65,
    /// The FX oracle rate is older than the allowed staleness window.
    StalePrice = 66,
    /// No FX oracle contract has been configured.
    FxOracleNotSet = 67,
}
