//! Data types for the invoice escrow contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config.
    Config,
    /// Persistent: escrow data by invoice id.
    Escrow(soroban_sdk::Symbol),
    /// Persistent: funder amounts by (invoice_id, funder_address).
    FunderAmount(soroban_sdk::Symbol, soroban_sdk::Address),
    /// Persistent: highest nonce consumed for a signed off-chain approval, by buyer address.
    Nonce(soroban_sdk::Address),
    /// Persistent: buyer whitelist flag by buyer address.
    BuyerWhitelist(soroban_sdk::Address),
    /// Persistent: funding invoice by BytesN<32> invoice id (new position management).
    Invoice(soroban_sdk::BytesN<32>),
    /// Persistent: investor position by (invoice_id BytesN<32>, investor address).
    InvestorPosition(soroban_sdk::BytesN<32>, soroban_sdk::Address),
    /// Instance: emergency multi-sig admin configuration.
    EmergencyConfig,
    /// Persistent: approvals collected for a given invoice's emergency release.
    EmergencyApprovals(soroban_sdk::Symbol),
    /// Persistent: pending two-admin settlement proposal by invoice id.
    SettlementProposal(soroban_sdk::BytesN<32>),
    /// Instance: total count of escrows created for indexing.
    EscrowCount,
    /// Persistent: invoice_id indexed by sequential creation order.
    EscrowIdByIndex(u32),
    /// Persistent: invoice metadata and parameters by BytesN<32>.
    InvoiceRecord(soroban_sdk::BytesN<32>),
    /// Instance: platform fee rate (bps) for a given invoice category.
    CategoryFee(InvoiceCategory),
    /// Instance: hard-capped maximum number of investors per registered invoice.
    MaxInvestors,
    /// Persistent: unique investor count for a registered invoice.
    InvestorCount(soroban_sdk::BytesN<32>),
    /// Persistent: dispute metadata for an invoice, by invoice id.
    Dispute(soroban_sdk::Symbol),
    /// Persistent: installment repayment milestone schedule by invoice id.
    InstallmentSchedule(soroban_sdk::Symbol),
    /// Instance: pending admin parameter change awaiting timelock expiry.
    PendingParamChange,
    /// Persistent: secondary-market buy limit by invoice id.
    BuyLimit(soroban_sdk::BytesN<32>),
    /// Persistent: a wallet's current buy-limit window by (invoice_id, wallet).
    BuyWindow(soroban_sdk::BytesN<32>, soroban_sdk::Address),
    /// Instance: FX oracle contract used to normalise foreign-currency investments.
    FxOracle,
    /// Instance: payment token for a supported investment currency.
    Currency(soroban_sdk::Symbol),
    /// Persistent: multi-currency investment records by (invoice_id, investor).
    InvestRecords(soroban_sdk::BytesN<32>, soroban_sdk::Address),
}

/// One multi-currency investment, normalised to the base currency at the FX
/// oracle rate in effect when it was made.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestmentRecord {
    /// Currency the investor paid in (e.g. USDC, EURC, NGNC).
    pub currency: soroban_sdk::Symbol,
    /// Amount paid, in the investment currency.
    pub amount: i128,
    /// Oracle rate applied (base units per currency unit, scaled by `decimals`).
    pub rate: i128,
    /// Decimal precision of `rate`.
    pub decimals: u32,
    /// Amount credited to the investor position, in base currency.
    pub base_amount: i128,
    /// Ledger timestamp of the investment.
    pub timestamp: u64,
}

/// Registered invoice metadata and funding parameters stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceData {
    /// Invoice identifier (32 bytes).
    pub invoice_id: soroban_sdk::BytesN<32>,
    /// Face value: total amount owed by debtor.
    pub face_value: i128,
    /// Funding target: total funding amount to raise.
    pub funding_target: i128,
    /// Yield rate in basis points (1 to 5000).
    pub yield_bps: u32,
    /// Funding deadline (ledger sequence).
    pub deadline_ledger: u32,
    /// Total amount raised so far from investors.
    pub total_raised: i128,
    /// Current lifecycle status.
    pub status: EscrowStatus,
    /// List of investor addresses.
    pub investors: soroban_sdk::Vec<soroban_sdk::Address>,
    /// Off-chain invoice document SHA256 hash for verification.
    pub document_hash: soroban_sdk::BytesN<32>,
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address (fee updates, platform recipient).
    pub admin: soroban_sdk::Address,
    /// Platform fee in basis points (e.g. 300 = 3%).
    pub fee_bps: u32,
    /// Optional payment distributor contract used for settlement/refund fan-out.
    pub payment_distributor: Option<soroban_sdk::Address>,
    /// Optional referral contract called on each investment to accrue referral fees.
    pub referral_contract: Option<soroban_sdk::Address>,
    /// Emergency pause flag for lifecycle-changing operations.
    pub paused: bool,
    /// When true, `fund_escrow` requires the buyer to be on the whitelist.
    /// Defaults to false (opt-in) so existing deployments/tests are unaffected
    /// until an admin explicitly enables it.
    pub whitelist_enabled: bool,
    /// Minimum investment amount (stroops) accepted by `fund_escrow`.
    /// `0` disables the floor (only `amount > 0` is required). Completing the
    /// remaining capacity below this floor is always allowed.
    pub min_investment: i128,
    /// Grace window (seconds) added to `due_dt` before an overdue invoice is
    /// locked out of settlement / becomes refund-eligible. See `record_payment`
    /// and `refund_escrow`.
    pub grace_period_seconds: u64,
    /// How long a `Disputed` escrow may sit unresolved before `resolve_dispute`
    /// falls back to refunding the buyer regardless of `favour`. Defaults to
    /// 604800 (7 days).
    pub dispute_timeout_secs: u64,
    /// Optional accreditation check callback contract. If set, `fund_escrow` calls
    /// this contract to verify investor eligibility before allowing funding.
    pub accreditation_callback: Option<soroban_sdk::Address>,
    /// Penalty interest rate in basis points (0..=10000) charged on payments
    /// received after due_date but within grace_period. Defaults to 0 (disabled).
    pub penalty_interest_bps: u32,
}

/// Lifecycle status of an escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    /// Created, awaiting funding.
    Created = 0,
    /// Funded by investor.
    Funded = 1,
    /// Payment recorded and distributed.
    Settled = 2,
    /// Refunded to investor after due date.
    Refunded = 3,
    /// Cancelled by seller while in Created state (refunds partial funders if any).
    /// Cancelled by seller while still in Created state and never funded
    /// (locked out once any investor contribution has been received).
    Cancelled = 4,
    /// A dispute has been raised; settlement/refund are held pending
    /// `resolve_dispute` (or its timeout fallback).
    Disputed = 5,
}

/// Commercial invoicing sector, used to look up a per-category platform fee
/// rate (see `Config`'s sibling storage at `StorageKey::CategoryFee`).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceCategory {
    /// Standard trade credit. The default when no category is specified.
    Standard = 0,
    /// Invoice factoring.
    Factoring = 1,
    /// Reverse factoring (supply-chain financing initiated by the debtor).
    Reverse = 2,
    /// Government contracts.
    Government = 3,
}

/// Per-category platform fee rate, set by the admin via `set_category_fee`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryFeeSchedule {
    /// Fee in basis points (0..=10000) applied to escrows in this category.
    pub fee_bps: u32,
}

/// Dispute metadata for an escrow currently (or previously) in the
/// `Disputed` status. Overwritten by each new `raise_dispute` call; a
/// resolved dispute's record is kept (with `resolved: true`) rather than
/// cleared, so it remains available for audit/history queries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeData {
    /// Who raised the dispute (buyer/debtor or seller).
    pub raiser: soroban_sdk::Address,
    /// Free-form reason/evidence reference (e.g. an off-chain document hash or URI).
    pub reason: soroban_sdk::Bytes,
    /// Ledger timestamp the dispute was raised at.
    pub raised_at: u64,
    /// Whether `resolve_dispute` has already run for this dispute.
    pub resolved: bool,
}

/// Per-invoice escrow data stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    /// Invoice identifier (Symbol, ≤10 chars when used as key).
    pub inv_id: soroban_sdk::Symbol,
    /// Seller (invoice owner).
    pub seller: soroban_sdk::Address,
    /// Debtor (authorized payer of the invoice).
    pub debtor: soroban_sdk::Address,
    /// Face value: what the debtor owes (amount to be paid at settlement).
    pub face_value: i128,
    /// Purchase price: total amount to be funded by all investors (discount applied here).
    pub purchase_price: i128,
    /// Total amount funded so far by all investors.
    pub funded_amt: i128,
    /// Primary funder address (MVP: single funder for now).
    pub funder: Option<soroban_sdk::Address>,
    /// Every funder that has contributed to this escrow so cleanup can prune their
    /// contribution records once the escrow reaches a terminal state.
    pub funders: soroban_sdk::Vec<soroban_sdk::Address>,
    /// Due date (ledger timestamp).
    pub due_dt: u64,
    /// Payment token contract address.
    pub token: soroban_sdk::Address,
    /// Allowed payment tokens; the first contribution locks `token` to one of these.
    pub accepted_tokens: soroban_sdk::Vec<soroban_sdk::Address>,
    /// Invoice token contract address (ownership/claim).
    pub inv_token: soroban_sdk::Address,
    /// Amount already paid by payer.
    pub paid_amt: i128,
    /// Current status.
    pub status: EscrowStatus,
    /// Minimum chunk size required for each partial funding operation (except the final one).
    pub funding_milestone: Option<i128>,
    /// Commitment hash: immutable on-chain anchor for off-chain invoice data (PDF hash, ERP ID, etc.).
    /// Set at creation, cannot be modified. SHA-256 hash (32 bytes).
    pub commitment: soroban_sdk::BytesN<32>,
    /// Discount in basis points applied to `face_value` when payment is early.
    /// `0` means the hook is not configured. A 0-bps discount is rejected by
    /// `set_early_settlement` as a no-op, so 0 is unambiguous as "unset".
    ///
    /// Stored as two scalar fields rather than an
    /// `Option<EarlySettlementConfig>` because soroban-sdk 27's
    /// `#[contracttype]` derive cannot convert a nested custom struct inside an
    /// `Option` — it emits an XDR conversion requiring
    /// `From<EarlySettlementConfig> for ScVal`, which the derive never
    /// generates. That made the type fail to build whenever the `testutils`
    /// feature was unified in, which is what any consumer with a dev-dependency
    /// on this crate ends up doing.
    pub early_settlement_discount_bps: u32,
    /// Ledger timestamp cutoff (exclusive) for the early-settlement discount.
    /// Only meaningful when `early_settlement_discount_bps` is non-zero.
    pub early_settlement_cutoff: u64,
    /// Commercial invoicing sector. Selects which `CategoryFeeSchedule` (if any)
    /// overrides `Config::fee_bps` for this escrow. Defaults to `Standard`.
    pub category: InvoiceCategory,
    /// Timestamp when escrow was fully funded (0 if not yet funded).
    pub funded_dt: u64,
}

/// A settlement awaiting approval from an admin other than its proposer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementProposal {
    pub proposer: soroban_sdk::Address,
    pub repayment_amount: i128,
}

/// Status for BytesN<32> funding invoices (position management).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceStatus {
    Open = 0,
    Funded = 1,
}

/// Funding invoice for secondary market position management (BytesN<32> invoices).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingInvoice {
    /// Seller who will receive funds on finalisation.
    pub seller: soroban_sdk::Address,
    /// Total funding target.
    pub funding_target: i128,
    /// Total raised so far across all investors.
    pub total_raised: i128,
    /// Ledger deadline for funding (partial_refund only before this).
    pub deadline_ledger: u32,
    /// Minimum investment floor for remaining position after partial refund.
    pub min_investment: i128,
    /// Optional per-investor cap applied on top_up.
    pub per_investor_cap: Option<i128>,
    /// Current status.
    pub status: InvoiceStatus,
    /// Payment token contract address.
    pub token: soroban_sdk::Address,
    /// Minimum funding threshold: `release_to_issuer` only succeeds once
    /// `total_raised` has reached this value.
    pub min_funding_threshold: i128,
    /// Maximum funding threshold: hard cap on `total_raised`. `invest` and
    /// `top_up` reject amounts that would push the total above it.
    pub max_funding_threshold: i128,
    /// Whether `ThresholdReached` has already been emitted for this invoice.
    /// Guarantees the event fires exactly once per invoice.
    pub threshold_reached: bool,
}

/// Funding progress view returned by `get_funding_status`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingStatus {
    /// Total amount raised so far across all investors.
    pub raised: i128,
    /// Minimum funding threshold configured for the invoice.
    pub min_threshold: i128,
    /// Maximum funding threshold configured for the invoice.
    pub max_threshold: i128,
    /// Percentage of the funding target filled, in basis points
    /// (10_000 = 100%). Capped at 10_000 when the target is exceeded.
    pub percent_filled_bps: u32,
}
/// Multi-signature configuration for emergency releases.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiSigConfig {
    /// Set of admin addresses authorized to approve emergency releases.
    pub admins: soroban_sdk::Vec<soroban_sdk::Address>,
    /// Number of approvals required to trigger the emergency release (N-of-M).
    pub threshold: u32,
}

/// Tracks which admins have approved an emergency release for a given invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyApprovals {
    /// List of admin addresses that have already approved this release.
    pub approvals: soroban_sdk::Vec<soroban_sdk::Address>,
}

/// One repayment milestone in an installment settlement schedule.
///
/// Milestones are ordered by `index` and their `cumulative_amount` values are
/// strictly increasing, ending at exactly `EscrowData::face_value`. A milestone
/// becomes `settled` once the escrow's cumulative `paid_amt` reaches its
/// `cumulative_amount`, or once the escrow itself reaches the terminal
/// `Settled` status (e.g. via an early-settlement discount or emergency
/// release that pays less than the undiscounted face value).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallmentMilestone {
    /// 0-based position of this milestone within the schedule.
    pub index: u32,
    /// Cumulative repayment target: sum of all installment amounts through
    /// this milestone (inclusive).
    pub cumulative_amount: i128,
    /// Ledger timestamp by which `cumulative_amount` must have been repaid.
    pub due_ts: u64,
    /// Whether this milestone has already been reached by recorded payments.
    pub settled: bool,
}

/// Seller-supplied entry used to configure an installment schedule.
///
/// `amount` is the size of the individual installment (not the cumulative
/// total); the contract derives and stores cumulative amounts when the
/// schedule is saved.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallmentInput {
    /// Ledger timestamp by which this installment is due.
    pub due_ts: u64,
    /// Size of this individual installment. Must be `> 0`.
    pub amount: i128,
}

/// A pending admin parameter change gated by a timelock (#443).
///
/// Critical parameter updates (fee_bps, grace_period, min_investment, etc.)
/// are proposed via `propose_param_change` and can only be executed after
/// `timelock_secs` have elapsed, giving participants time to react.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingParamChange {
    /// Which parameter is being changed.
    pub param: ParamType,
    /// The new value to apply.
    pub new_value: i128,
    /// Ledger timestamp when the change was proposed.
    pub proposed_at: u64,
    /// Minimum seconds that must elapse before execution.
    pub timelock_secs: u64,
    /// Address that proposed the change.
    pub proposer: soroban_sdk::Address,
}

/// Identifies which admin parameter a timelocked change targets.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ParamType {
    FeeBps = 0,
    GracePeriod = 1,
    MinInvestment = 2,
    DisputeTimeout = 3,
    PenaltyInterestBps = 4,
}

/// Optional early-settlement discount hook configuration.
///
/// If set on an escrow, any `record_payment` whose ledger timestamp is strictly
/// before `cutoff_date` will have the effective face value reduced by
/// `discount_bps` basis points (e.g. 200 = 2%).  The discounted effective face
/// value is floored to `1` so the escrow can always be settled.
///
/// The discount applies uniformly across all partial payments made before the
/// cutoff; payments made on or after the cutoff are settled at the original
/// `face_value`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarlySettlementConfig {
    /// Discount in basis points applied to `face_value` when payment is early.
    /// Must be in [1, 9999] — a 0-bps discount is a no-op; 10000 bps (100%) is
    /// rejected to prevent the effective face value from collapsing to 0.
    pub discount_bps: u32,
    /// Ledger timestamp deadline (exclusive): payment is "early" iff
    /// `current_timestamp < cutoff_date`.
    pub cutoff_date: u64,
}
