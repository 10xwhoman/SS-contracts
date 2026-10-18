use soroban_sdk::{contracttype, Address, Vec};

use crate::errors::Error;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Distribution(soroban_sdk::Address, soroban_sdk::Symbol),
    /// Ordered platform fee tiers.
    FeeTiers,
    /// Investor bonus rate in basis points.
    InvestorBonusBps,
    /// Fee recipient address for platform fees (Issue #122).
    FeeRecipient,
    /// Temporary: re-entrancy guard flag for distribution entrypoints. Only
    /// needs to be readable within a single invocation. (Issue #127, #492)
    Locked,
    /// Whitelisted escrow contract address allowed to call distribute_payment (Issue #121).
    EscrowContract,
    /// Role admin for a given role (Issue #182).
    RoleAdmin(soroban_sdk::Symbol),
    /// Role grant flag for a given (role, account) pair (Issue #182).
    RoleGrant(soroban_sdk::Symbol, soroban_sdk::Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionState {
    pub paid_distributed: i128,
    pub refund_distributed: bool,
}

/// Platform fee rate for a bounded payment range.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeTier {
    pub min_amount: i128,
    pub max_amount: i128,
    pub fee_bps: u32,
}

/// Maximum allowed fee in basis points (100% = 10,000 BPS). Issue #124.
#[allow(dead_code)]
pub const MAX_FEE_BPS: u32 = 10_000;

/// Distribution split configuration with an optional referral fee cut. Issue #130.
///
/// The `recipients` and `shares_bps` vectors are parallel: `shares_bps[i]` is the
/// basis-point share of `recipients[i]`. An optional `referral` recipient receives
/// `referral_bps` taken off the top before the recipient shares are paid.
///
/// `recipients[0]` is the primary/residual recipient: it always receives the exact
/// remainder (`amount - referral_cut - sum(shares of recipients[1..]))`, so the full
/// amount is distributed with no dust left in the contract, mirroring the existing
/// "seller absorbs rounding" behavior of `distribute_payment`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionSplit {
    /// Primary recipients of the distribution. `recipients[0]` is the residual recipient.
    pub recipients: Vec<Address>,
    /// Basis-point share for each recipient, aligned with `recipients`.
    pub shares_bps: Vec<u32>,
    /// Optional referral recipient that receives a cut before the primary splits.
    pub referral: Option<Address>,
    /// Referral cut in basis points (0..=10,000), taken from the total.
    pub referral_bps: u32,
}

impl DistributionSplit {
    /// Total basis points allocated by this split (referral cut + all recipient shares).
    /// Saturates on overflow; any real overflow is far beyond `MAX_FEE_BPS` and is
    /// therefore rejected by `validate`.
    pub fn total_bps(&self) -> u32 {
        let mut total = self.referral_bps;
        for share in self.shares_bps.iter() {
            total = total.saturating_add(share);
        }
        total
    }

    /// Validate the split configuration. Issue #130.
    pub fn validate(&self) -> Result<(), Error> {
        if self.recipients.is_empty() || self.recipients.len() != self.shares_bps.len() {
            return Err(Error::InvalidSplit);
        }
        if self.referral_bps > 0 && self.referral.is_none() {
            return Err(Error::InvalidReferralCut);
        }
        if self.referral_bps > MAX_FEE_BPS {
            return Err(Error::InvalidReferralCut);
        }
        if self.total_bps() > MAX_FEE_BPS {
            return Err(Error::SplitsExceedTotal);
        }
        Ok(())
    }
}

/// A single asset routing entry for multi-currency distribution. Issue #126.
///
/// Distributes `amount` of `token` (already held by this contract) across `split`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRoute {
    pub token: Address,
    pub amount: i128,
    pub split: DistributionSplit,
}

/// Dry-run preview of a `distribute_payment` split, with no state mutation or
/// token transfers. Issue #129.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionPreview {
    pub seller_amount: i128,
    pub investor_amount: i128,
    pub platform_fee: i128,
    pub total_distribution: i128,
}

/// A single entry in a batch payment fanout.
///
/// Each entry represents one settled-payment distribution from a single escrow invoice.
/// The distributor contract must already hold the tokens for every entry before
/// `distribute_batch` is called.
///
/// Field names are kept ≤10 chars to satisfy Soroban's `contracttype` constraint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPaymentEntry {
    /// Escrow contract that authorises this distribution.
    pub escrow: Address,
    /// Invoice identifier within the escrow.
    pub inv_id: soroban_sdk::Symbol,
    /// Payment token contract.
    pub token: Address,
    /// Seller (invoice owner) — receives the face-value portion.
    pub seller: Address,
    /// Investor (funder) — receives the investor portion.
    pub funder: Address,
    /// Platform admin — receives the fee.
    pub admin: Address,
    /// Cumulative paid amount for this invoice (used to detect double-distribution).
    pub paid_amt: i128,
    /// Net amount to pay the seller for this settlement call.
    pub seller_amt: i128,
    /// Net amount to pay the investor for this settlement call.
    pub investor_amt: i128,
    /// Platform fee for this settlement call.
    pub fee_amt: i128,
    /// Escrow status after the payment (must be Funded=1 or Settled=2).
    pub status: u32,
}
