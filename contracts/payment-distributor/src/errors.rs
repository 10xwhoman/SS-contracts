use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInit = 1,
    NotInit = 2,
    InvalidAmount = 3,
    Unauthorized = 4,
    InvalidEscrowStatus = 5,
    NothingToDistribute = 6,
    RefundAlreadyDistributed = 7,
    Overflow = 8,
    WrongDistributor = 9,
    /// Fee BPS exceeds maximum allowed value (10,000). Issue #124.
    InvalidBps = 10,
    /// A re-entrant call into a guarded entrypoint was detected. Issue #127.
    ReentrancyDetected = 11,
    /// A referral cut was requested without a valid referral recipient. Issue #130.
    InvalidReferralCut = 12,
    /// Referral cut plus recipient shares exceed the total (10,000 BPS). Issue #130.
    SplitsExceedTotal = 13,
    /// A distribution split is malformed (empty or mismatched recipients/shares). Issue #130.
    InvalidSplit = 14,
    /// No assets were supplied to a multi-asset distribution. Issue #126.
    EmptyAssetList = 15,
    /// The same asset appears more than once in a multi-asset distribution. Issue #126.
    AssetMismatch = 16,
    /// The contract holds no balance to withdraw. Issue #125.
    NothingToWithdraw = 17,
    /// The calling escrow contract does not match the whitelisted escrow address. Issue #131.
    UnauthorizedEscrow = 18,
    /// The contract balance is insufficient to distribute the requested amount. Issue #120.
    InsufficientBalance = 19,
    /// The contract holds no dust balance to sweep. Issue #119.
    NothingToSweep = 20,
    /// Batch must contain at least one entry.
    EmptyBatch = 21,
    /// Batch exceeds the maximum allowed number of entries.
    BatchTooLarge = 22,
    /// Fee fanout contains more recipients than allowed.
    TooManyFeeRecipients = 23,
    /// Fee fanout amounts are invalid.
    InvalidFeeSplit = 24,
    /// Investor bonus rate exceeds the maximum.
    InvalidBonusRate = 25,
    /// Refund contains more funders than allowed.
    TooManyRefundRecipients = 26,
    /// Refund weights are invalid.
    InvalidRefundWeight = 27,
    /// Nothing left in the distributor to refund. Issue #482.
    NothingToRefund = 28,
}
