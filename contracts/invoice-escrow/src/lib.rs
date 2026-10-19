//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod buy_limit;
mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, BytesN, Env, IntoVal, Symbol, Vec};

use types::{MultiSigConfig, SettlementProposal};

// EscrowStatus is re-exported publicly; Config, EscrowData, and InvoiceData are crate-private.
pub use types::EscrowStatus;
pub use types::FundingStatus;
pub use types::InstallmentMilestone;
pub use types::InvoiceCategory;
use types::{
    CategoryFeeSchedule, Config, DisputeData, EscrowData, FundingInvoice, InstallmentInput,
    InvestmentRecord, InvoiceData, InvoiceStatus,
};

pub use errors::Error;

/// Percentage of the funding target filled, expressed in basis points
/// (10_000 = 100%). Returns 0 for a non-positive target or raised amount and
/// caps at 10_000 when the target is exceeded.
fn percent_filled_bps(raised: i128, target: i128) -> u32 {
    if target <= 0 || raised <= 0 {
        return 0;
    }
    let bps = raised.saturating_mul(10_000) / target;
    core::cmp::min(bps, 10_000) as u32
}

/// Reject the zero address (all-zero 32-byte Ed25519 key) which is never a valid participant.
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    // Convert address to its string representation and check for the well-known
    // zero account (all 32 bytes are 0x00).  The StrKey encoding of the zero
    // account is the constant below.
    let zero_str = soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

const MAX_BPS: u32 = 10_000;
const MAX_SETTLEMENT_FEE_BPS: u32 = 1_000;
const MAX_INVESTORS_HARD_LIMIT: u32 = 500;
const DISTRIBUTE_PAYMENT_FN: &str = "distribute_payment";
const DISTRIBUTE_REFUND_FN: &str = "distribute_refund";

/// Minimum escrow duration: 1 hour (3600 seconds).
const MIN_ESCROW_DURATION_SECS: u64 = 3_600;
/// Maximum escrow duration: 365 days (31,536,000 seconds).
const MAX_ESCROW_DURATION_SECS: u64 = 31_536_000;

/// Default dispute resolution timeout: 7 days (604,800 seconds).
const DEFAULT_DISPUTE_TIMEOUT_SECS: u64 = 604_800;

/// Maximum number of milestones allowed in one installment repayment schedule.
/// Bounds storage growth and per-payment iteration cost.
const MAX_INSTALLMENTS: u32 = 64;

#[contract]
pub struct InvoiceEscrow;

fn ensure_not_paused(config: &Config) -> Result<(), Error> {
    if config.paused {
        return Err(Error::Paused);
    }
    Ok(())
}

/// Calculate early repayment yield rebate for investors when invoice is paid before due date.
/// Rebate is proportional to the time remaining until maturity: fewer days means higher rebate.
/// Returns the rebate amount to deduct from investor distribution.
fn calculate_early_repayment_rebate(
    current_ts: u64,
    due_dt: u64,
    funded_dt: u64,
    investor_amount: i128,
) -> i128 {
    if current_ts >= due_dt || funded_dt == 0 {
        return 0;
    }
    let total_loan_period = due_dt.saturating_sub(funded_dt);
    if total_loan_period == 0 {
        return 0;
    }
    let days_remaining = due_dt.saturating_sub(current_ts);
    let rebate_bps = days_remaining
        .saturating_mul(10_000)
        .checked_div(total_loan_period)
        .unwrap_or(0) as i128;
    investor_amount
        .checked_mul(rebate_bps)
        .unwrap_or(0)
        .checked_div(10_000)
        .unwrap_or(0)
}

/// Mark every installment milestone whose cumulative target has been reached by
/// `paid_amt` as settled, emitting `installment_settled` for each one that
/// flips. When `fully_settled` is true (escrow reached terminal `Settled`),
/// any remaining milestones are settled unconditionally so discounted or
/// emergency settlements still close out the schedule.
///
/// No-op when the invoice has no configured schedule.
fn advance_installment_schedule(
    env: &Env,
    invoice_id: &Symbol,
    paid_amt: i128,
    fully_settled: bool,
) {
    let Some(mut schedule) = storage::get_installment_schedule(env, invoice_id) else {
        return;
    };
    let mut changed = false;
    for i in 0..schedule.len() {
        let Some(mut milestone) = schedule.get(i) else {
            break;
        };
        if milestone.settled {
            continue;
        }
        if fully_settled || paid_amt >= milestone.cumulative_amount {
            milestone.settled = true;
            schedule.set(i, milestone.clone());
            changed = true;
            events::installment_settled(
                env,
                invoice_id.clone(),
                milestone.index,
                milestone.cumulative_amount,
                paid_amt,
            );
        } else {
            // Milestones are stored in strictly increasing cumulative order,
            // so no later milestone can have been reached either.
            break;
        }
    }
    if changed {
        storage::set_installment_schedule(env, invoice_id, &schedule);
    }
}

#[contractimpl]
impl InvoiceEscrow {
    /// Initialize the contract with admin and platform fee (basis points, e.g. 300 = 3%).
    pub fn initialize(env: Env, admin: Address, platform_fee_bps: u32) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if platform_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let config = Config {
            admin: admin.clone(),
            fee_bps: platform_fee_bps,
            payment_distributor: None,
            referral_contract: None,
            paused: false,
            whitelist_enabled: false,
            min_investment: 0,
            grace_period_seconds: 0,
            dispute_timeout_secs: DEFAULT_DISPUTE_TIMEOUT_SECS,
            accreditation_callback: None,
            penalty_interest_bps: 0,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Set the per-invoice unique investor limit. The hard ceiling keeps
    /// settlement iteration bounded even when the admin changes the default.
    pub fn set_max_investors(env: Env, count: u32) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        if !(1..=MAX_INVESTORS_HARD_LIMIT).contains(&count) {
            return Err(Error::InvalidMaxInvestors);
        }
        storage::set_max_investors(&env, count);
        events::max_investors_updated(&env, count);
        Ok(())
    }

    /// Admin-only: set the grace period (seconds) added to `due_dt` before an
    /// overdue invoice is locked out of settlement / becomes refund-eligible.
    pub fn set_grace_period(
        env: Env,
        admin: Address,
        grace_period_seconds: u64,
    ) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        let old = config.grace_period_seconds;
        config.grace_period_seconds = grace_period_seconds;
        storage::set_config(&env, &config);
        events::grace_period_updated(&env, old, grace_period_seconds);
        Ok(())
    }

    /// Admin-only: set the platform fee (bps, 0..=10000) for an invoice category.
    pub fn set_category_fee(
        env: Env,
        admin: Address,
        category: InvoiceCategory,
        fee_bps: u32,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        storage::set_category_fee(&env, category, &CategoryFeeSchedule { fee_bps });
        events::category_fee_updated(&env, category, fee_bps);
        Ok(())
    }

    /// Set the fallback settlement fee, paid to the configured admin treasury.
    /// Category-specific fee schedules continue to override this rate.
    pub fn set_settlement_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if fee_bps > MAX_SETTLEMENT_FEE_BPS {
            return Err(Error::FeeTooHigh);
        }
        let old_fee_bps = config.fee_bps;
        config.fee_bps = fee_bps;
        storage::set_config(&env, &config);
        events::platform_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    /// View: the configured fee (bps) for `category`, or `Config::fee_bps` if
    /// no category-specific rate has been set.
    pub fn get_category_fee(env: Env, category: InvoiceCategory) -> Result<u32, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_category_fee(&env, category)
            .map(|s| s.fee_bps)
            .unwrap_or(config.fee_bps))
    }

    /// Admin-only: set the minimum investment amount for `fund_escrow`.
    /// Pass `0` to disable the floor (deposits must still be strictly positive).
    pub fn set_min_investment(env: Env, admin: Address, min_investment: i128) -> Result<(), Error> {
        admin.require_auth();
        if min_investment < 0 {
            return Err(Error::InvalidAmount);
        }
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.min_investment = min_investment;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: enable/disable buyer whitelist enforcement on `fund_escrow`.
    pub fn set_whitelist_enabled(env: Env, admin: Address, enabled: bool) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.whitelist_enabled = enabled;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: set the optional accreditation check callback contract.
    /// The callback is invoked before `fund_escrow` to verify investor eligibility.
    pub fn set_accreditation_callback(
        env: Env,
        admin: Address,
        callback: Option<Address>,
    ) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.accreditation_callback = callback;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: set penalty interest rate (basis points) charged on late payments.
    /// Valid range: 0..=10000. Set to 0 to disable penalty interest.
    pub fn set_penalty_interest_bps(
        env: Env,
        admin: Address,
        penalty_bps: u32,
    ) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if penalty_bps > MAX_BPS {
            return Err(Error::InvalidPenaltyConfig);
        }
        config.penalty_interest_bps = penalty_bps;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: add or remove a buyer from the whitelist.
    pub fn set_buyer_whitelisted(
        env: Env,
        admin: Address,
        buyer: Address,
        allowed: bool,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::set_whitelisted(&env, &buyer, allowed);
        Ok(())
    }

    /// View: is `buyer` whitelisted to fund escrows.
    pub fn is_buyer_whitelisted(env: Env, buyer: Address) -> bool {
        storage::is_whitelisted(&env, &buyer)
    }

    /// View: return the configured accreditation callback contract, if set.
    pub fn get_accreditation_callback(env: Env) -> Result<Option<Address>, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.accreditation_callback)
    }

    /// View: return the configured penalty interest rate (basis points).
    pub fn get_penalty_interest_bps(env: Env) -> Result<u32, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.penalty_interest_bps)
    }

    /// Create a single-token escrow for legacy callers. Caller (seller) must be authenticated.
    /// face_value: what the debtor owes (amount to be paid at settlement)
    /// purchase_price: what the investor pays (discount applied here)
    /// commitment: immutable on-chain anchor (SHA-256 hash of off-chain invoice data)
    pub fn create_escrow_legacy(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        debtor: Address,
        face_value: i128,
        purchase_price: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
        commitment: soroban_sdk::BytesN<32>,
        funding_milestone: Option<i128>,
        category: Option<InvoiceCategory>,
    ) -> Result<(), Error> {
        seller.require_auth();
        if face_value <= 0 || purchase_price <= 0 {
            return Err(Error::InvalidAmount);
        }
        if due_date == 0 {
            return Err(Error::InvalidDueDate);
        }
        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(Error::InvalidDueDate);
        }
        let duration = due_date.saturating_sub(current_timestamp);
        if !(MIN_ESCROW_DURATION_SECS..=MAX_ESCROW_DURATION_SECS).contains(&duration) {
            return Err(Error::InvalidDuration);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        // Ensure the payment token and invoice token use the same decimals to avoid
        // settlement/rounding mismatches during distribution and fee calculations.
        let inv_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &invoice_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        let pay_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &payment_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        if let (Some(inv_d), Some(pay_d)) = (inv_decimals, pay_decimals) {
            if inv_d != pay_d {
                return Err(Error::InvalidAssetDecimals);
            }
        }
        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            debtor: debtor.clone(),
            face_value,
            purchase_price,
            funded_amt: 0,
            funder: None,
            funders: soroban_sdk::Vec::new(&env),
            due_dt: due_date,
            token: payment_token.clone(),
            accepted_tokens: soroban_sdk::vec![&env, payment_token.clone()],
            inv_token: invoice_token.clone(),
            paid_amt: 0,
            status: EscrowStatus::Created,
            funding_milestone,
            commitment: commitment.clone(),
            early_settlement_discount_bps: 0,
            early_settlement_cutoff: 0,
            category: category.unwrap_or(InvoiceCategory::Standard),
            funded_dt: 0,
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);

        // Store the invoice_id at the current index for pagination
        let current_count = storage::get_escrow_count(&env);
        storage::set_escrow_id_by_index(&env, current_count, &invoice_id);
        storage::increment_escrow_count(&env);

        events::escrow_created(
            &env,
            invoice_id.clone(),
            &seller,
            &debtor,
            face_value,
            purchase_price,
            due_date,
            &payment_token,
            &invoice_token,
            &commitment,
            data.funding_milestone,
        );
        events::escrow_status_changed(&env, invoice_id, EscrowStatus::Created, current_timestamp);
        Ok(())
    }

    /// Create an escrow with an explicit whitelist of acceptable funding tokens.
    /// The `create_escrow_legacy` entry point remains available and accepts only
    /// its `payment_token` argument.
    pub fn create_escrow(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        debtor: Address,
        face_value: i128,
        purchase_price: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
        commitment: soroban_sdk::BytesN<32>,
        funding_milestone: Option<i128>,
        category: Option<InvoiceCategory>,
        accepted_tokens: soroban_sdk::Vec<Address>,
    ) -> Result<(), Error> {
        if accepted_tokens.is_empty() || !accepted_tokens.iter().any(|token| token == payment_token)
        {
            return Err(Error::TokenNotAccepted);
        }
        let key = invoice_id.clone();
        Self::create_escrow_legacy(
            env.clone(),
            invoice_id,
            seller,
            debtor,
            face_value,
            purchase_price,
            due_date,
            payment_token,
            invoice_token,
            commitment,
            funding_milestone,
            category,
        )?;
        let mut data = storage::get_escrow(&env, key.clone()).ok_or(Error::EscrowNotFound)?;
        data.accepted_tokens = accepted_tokens;
        storage::set_escrow(&env, key, &data);
        Ok(())
    }

    /// Explicitly named alias for clients migrating from the legacy API.
    pub fn create_escrow_with_tokens(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        debtor: Address,
        face_value: i128,
        purchase_price: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
        commitment: soroban_sdk::BytesN<32>,
        funding_milestone: Option<i128>,
        category: Option<InvoiceCategory>,
        accepted_tokens: soroban_sdk::Vec<Address>,
    ) -> Result<(), Error> {
        Self::create_escrow(
            env, invoice_id, seller, debtor, face_value, purchase_price, due_date,
            payment_token, invoice_token, commitment, funding_milestone, category, accepted_tokens,
        )
    }

    /// Cancel an escrow in Created state, refunding any partial funds to the funders.
    /// Only the seller may cancel, and only while status is Created.
    /// Cancel an unfunded escrow. Only the seller may cancel, and only while status is Created
    /// AND no investor has contributed any funds yet.
    ///
    /// Locked out after partial payment: `fund_escrow` accepts partial contributions and only
    /// flips `status` to `Funded` once the escrow is fully subscribed, so an escrow with
    /// `funded_amt > 0` can still read as `Created`. Cancelling in that window would strand the
    /// investor's already-transferred funds (cancellation has no refund path), so any nonzero
    /// `funded_amt` blocks cancellation regardless of status.
    ///
    /// Emits `escrow_refunded` (if partial funds existed) and `escrow_cancelled`.
    pub fn cancel_escrow(env: Env, invoice_id: Symbol, seller: Address) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status == EscrowStatus::Funded {
            return Err(Error::EscrowFunded);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::CancelNotAllowed);
        }

        if data.funded_amt > 0 {
            let amount_to_refund = data.funded_amt;
            let token = token::Client::new(&env, &data.token);
            let contract = env.current_contract_address();
            let funder_opt = data.funder.clone();

            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (EscrowStatus::Cancelled as u32).into_val(&env)
                    ],
                );
            } else if let Some(funder) = &funder_opt {
                let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                if funder_amt > 0 {
                    token.transfer(&contract, funder, &funder_amt);
                }
            }

            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
            events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        }
        data.status = EscrowStatus::Cancelled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_cancelled(&env, invoice_id.clone(), &seller);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Cancelled,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Seller-only: attach or update the early-settlement discount hook for a Created or Funded escrow.
    ///
    /// Rules:
    /// - Only callable by the escrow's seller.
    /// - `discount_bps` must be in [1, 9999]. A zero discount is meaningless; 10 000 bps
    ///   (100%) would collapse the effective face value to zero, so it is rejected.
    /// - `cutoff_date` must be strictly in the future and must not exceed `due_dt`.
    /// - Cannot be set on an escrow that has already reached a terminal state
    ///   (Settled, Refunded, Cancelled).
    /// - Can be called multiple times to update the config (e.g., extend the window
    ///   or adjust the rate) as long as the escrow is still live.
    pub fn set_early_settlement(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        discount_bps: u32,
        cutoff_date: u64,
    ) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }

        // Terminal states: hook can no longer be meaningful
        match data.status {
            EscrowStatus::Settled | EscrowStatus::Refunded | EscrowStatus::Cancelled => {
                return Err(Error::InvalidEarlySettlement);
            }
            _ => {}
        }

        // Validate discount_bps: must be [1, 9999]
        if discount_bps == 0 || discount_bps >= MAX_BPS {
            return Err(Error::InvalidEarlySettlement);
        }

        // cutoff_date must be strictly in the future
        let now = env.ledger().timestamp();
        if cutoff_date <= now {
            return Err(Error::InvalidEarlySettlement);
        }
        // cutoff_date must not exceed due_dt (no discount window past maturity)
        if cutoff_date > data.due_dt {
            return Err(Error::InvalidEarlySettlement);
        }

        data.early_settlement_discount_bps = discount_bps;
        data.early_settlement_cutoff = cutoff_date;
        storage::set_escrow(&env, invoice_id, &data);
        Ok(())
    }

    /// Configure (or replace) the installment repayment milestone schedule for
    /// an invoice — Issue #450.
    ///
    /// The seller defines a sequence of future installments; the contract
    /// stores them as cumulative milestones that `record_payment` settles
    /// progressively as repayments arrive.
    ///
    /// Rules:
    /// - Only the escrow's seller may call this, and only while the escrow is
    ///   still `Created` or `Funded` with **zero** recorded repayment
    ///   (`paid_amt == 0`), so the schedule cannot be moved once the debtor
    ///   has started paying.
    /// - `1..=MAX_INSTALLMENTS` entries; each `amount` must be `> 0`.
    /// - `due_ts` values must be strictly increasing, strictly in the future,
    ///   and on or before the escrow's `due_dt`.
    /// - Installment amounts must sum **exactly** to `face_value`, so the
    ///   schedule always covers the full repayment obligation.
    ///
    /// Emits `installment_schedule_set`.
    pub fn set_installment_schedule(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        schedule: soroban_sdk::Vec<InstallmentInput>,
    ) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let data = storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }
        match data.status {
            EscrowStatus::Created | EscrowStatus::Funded => {}
            _ => return Err(Error::InvalidInstallmentSchedule),
        }
        if data.paid_amt > 0 {
            return Err(Error::InvalidInstallmentSchedule);
        }

        let count = schedule.len();
        if count == 0 || count > MAX_INSTALLMENTS {
            return Err(Error::InvalidInstallmentSchedule);
        }

        let now = env.ledger().timestamp();
        let mut milestones = soroban_sdk::Vec::new(&env);
        let mut cumulative: i128 = 0;
        let mut prev_due: u64 = 0;
        let mut first = true;
        for input in schedule.iter() {
            if input.amount <= 0 {
                return Err(Error::InvalidInstallmentSchedule);
            }
            if input.due_ts <= now || input.due_ts > data.due_dt {
                return Err(Error::InvalidInstallmentSchedule);
            }
            if !first && input.due_ts <= prev_due {
                return Err(Error::InvalidInstallmentSchedule);
            }
            first = false;
            prev_due = input.due_ts;
            cumulative = cumulative
                .checked_add(input.amount)
                .ok_or(Error::Overflow)?;
            milestones.push_back(InstallmentMilestone {
                index: milestones.len(),
                cumulative_amount: cumulative,
                due_ts: input.due_ts,
                settled: false,
            });
        }
        if cumulative != data.face_value {
            return Err(Error::InvalidInstallmentSchedule);
        }

        storage::set_installment_schedule(&env, &invoice_id, &milestones);
        events::installment_schedule_set(&env, invoice_id, count, prev_due, cumulative);
        Ok(())
    }

    /// Fund the escrow (investor buys part or all of the invoice at purchase_price).
    /// Transfers `amount` from buyer to this contract. Multiple investors can fund until fully subscribed.
    pub fn fund_escrow_legacy(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        buyer.require_auth();
        Self::fund_escrow_core(&env, invoice_id, &buyer, amount)
    }

    /// Fund using an explicitly selected token from the escrow whitelist.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        funding_token: Address,
    ) -> Result<(), Error> {
        buyer.require_auth();
        Self::fund_escrow_core_with_token(&env, invoice_id, &buyer, amount, &funding_token)
    }

    /// Explicitly named alias for clients migrating from the legacy API.
    pub fn fund_escrow_with_token(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        funding_token: Address,
    ) -> Result<(), Error> {
        Self::fund_escrow(env, invoice_id, buyer, amount, funding_token)
    }

    /// Fund the escrow on behalf of `buyer` using a signed off-chain approval that a relayer
    /// submits on their behalf. `buyer` authorizes exactly this `(invoice_id, amount, nonce, expiry)`
    /// tuple, and `nonce` must be strictly greater than the last nonce consumed by `buyer` so
    /// the same signed approval cannot be replayed.
    ///
    /// Issue #183: Includes an `expiry` timestamp. If the ledger timestamp exceeds `expiry`
    /// the signature is rejected, limiting the window for replay attacks.
    pub fn fund_escrow_signed(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        nonce: u64,
        expiry: u64,
    ) -> Result<(), Error> {
        buyer.require_auth_for_args((invoice_id.clone(), amount, nonce, expiry).into_val(&env));

        let current_ts = env.ledger().timestamp();
        if current_ts > expiry {
            return Err(Error::SignatureExpired);
        }

        let last_nonce = storage::get_nonce(&env, &buyer);
        if nonce <= last_nonce {
            return Err(Error::NonceAlreadyUsed);
        }

        Self::fund_escrow_core(&env, invoice_id.clone(), &buyer, amount)?;

        storage::set_nonce(&env, &buyer, nonce);
        events::escrow_funded_signed(&env, invoice_id, &buyer, amount, nonce);
        Ok(())
    }

    /// Shared funding logic used by both the directly-authorized and signed-approval entry points.
    fn fund_escrow_core(
        env: &Env,
        invoice_id: Symbol,
        buyer: &Address,
        amount: i128,
    ) -> Result<(), Error> {
        let data = storage::get_escrow(env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        Self::fund_escrow_core_with_token(env, invoice_id, buyer, amount, &data.token)
    }

    fn fund_escrow_core_with_token(
        env: &Env,
        invoice_id: Symbol,
        buyer: &Address,
        amount: i128,
        funding_token: &Address,
    ) -> Result<(), Error> {
        // Fail fast: validate amount before hitting storage.
        if amount == 0 {
            return Err(Error::ZeroAmount);
        }
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if config.whitelist_enabled && !storage::is_whitelisted(env, buyer) {
            return Err(Error::NotWhitelisted);
        }
        // Call accreditation callback if configured
        if let Some(callback) = &config.accreditation_callback {
            let is_accredited: bool = env
                .try_invoke_contract::<bool, soroban_sdk::Error>(
                    callback,
                    &Symbol::new(env, "is_accredited"),
                    soroban_sdk::vec![env, buyer.to_val()],
                )
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(false);
            if !is_accredited {
                return Err(Error::NotWhitelisted);
            }
        }

        let mut data = storage::get_escrow(env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }

        if !data
            .accepted_tokens
            .iter()
            .any(|token| token == *funding_token)
        {
            return Err(Error::TokenNotAccepted);
        }
        if data.funded_amt == 0 {
            data.token = funding_token.clone();
        } else if data.token != *funding_token {
            return Err(Error::TokenNotAccepted);
        }

        // Check that funding doesn't exceed purchase_price
        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        let remaining_to_fund = data
            .purchase_price
            .checked_sub(data.funded_amt)
            .ok_or(Error::Overflow)?;

        // Enforce global minimum investment to prevent dust deposits, except when
        // the funder is completing the exact remaining capacity.
        if config.min_investment > 0
            && amount != remaining_to_fund
            && amount < config.min_investment
        {
            return Err(Error::AmountBelowMinimum);
        }

        // Validate milestone constraints if a milestone is set
        if let Some(milestone) = data.funding_milestone {
            // Funder is always allowed to just fund exactly the remaining amount to complete the escrow.
            // If they are not completing the escrow, the amount must be at least the milestone and a multiple of it.
            if amount != remaining_to_fund && (amount < milestone || amount % milestone != 0) {
                return Err(Error::InvalidMilestoneAmount);
            }
        }

        let contract = env.current_contract_address();
        token::Client::new(env, funding_token).transfer(buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent their ownership share
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(env, "mint"),
            soroban_sdk::vec![env, buyer.to_val(), amount.into_val(env), contract.to_val()],
        );

        // Track this funder's contribution
        let current_funder_amt = storage::get_funder_amount(env, invoice_id.clone(), buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(env, invoice_id.clone(), buyer, new_funder_amt);

        data.funded_amt = new_funded;

        let mut already_recorded = false;
        for funder in data.funders.iter() {
            if funder == buyer.clone() {
                already_recorded = true;
                break;
            }
        }
        if !already_recorded {
            data.funders.push_back(buyer.clone());
        }

        // MVP: Store the first funder for direct distribution
        if data.funder.is_none() {
            data.funder = Some(buyer.clone());
        }

        // If fully funded, transition to Funded status and record funding timestamp
        if data.funded_amt == data.purchase_price {
            data.status = EscrowStatus::Funded;
            data.funded_dt = env.ledger().timestamp();
        }

        storage::set_escrow(env, invoice_id.clone(), &data);
        events::escrow_funded(
            env,
            invoice_id.clone(),
            buyer,
            amount,
            data.funded_amt,
            data.purchase_price,
        );
        if data.status == EscrowStatus::Funded {
            events::escrow_status_changed(
                env,
                invoice_id.clone(),
                EscrowStatus::Funded,
                env.ledger().timestamp(),
            );
        }

        // Best-effort referral fee accrual: if a referral contract is configured,
        // notify it of this investment so it can accrue rewards to the buyer's referrer.
        // Errors from the referral contract are swallowed so a referral misconfiguration
        // never blocks a legitimate investment.
        if let Some(referral_addr) = config.referral_contract.as_ref() {
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                referral_addr,
                &Symbol::new(env, "accrue_referral_fee"),
                soroban_sdk::vec![
                    env,
                    contract.to_val(),
                    invoice_id.into_val(env),
                    buyer.to_val(),
                    amount.into_val(env),
                ],
            );
        }

        Ok(())
    }

    /// Record payment: distribute to investors and platform fee. Payer must auth.
    /// Payer must be the authorized debtor for this invoice.
    /// Payment is applied toward face_value; fees are calculated on the payment amount.
    /// MVP: Distributes pro-rata to all funders based on their contribution.
    pub fn record_payment(
        env: Env,
        invoice_id: Symbol,
        payer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        payer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Enforce payer role: payer must be the authorized debtor
        if payer != data.debtor {
            return Err(Error::InvalidPayer);
        }

        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }

        // Deadline verification: payment is still acceptable through the end of
        // the grace window, not just up to the bare due date -- see #375.
        let current_ts = env.ledger().timestamp();
        if current_ts > data.due_dt.saturating_add(config.grace_period_seconds) {
            return Err(Error::EscrowOverdue);
        }

        // Compute effective face value: apply early-settlement discount if the hook
        // is configured and the payment arrives strictly before the cutoff date.
        let effective_face_value = if data.early_settlement_discount_bps > 0
            && current_ts < data.early_settlement_cutoff
        {
            let discount = data
                .face_value
                .checked_mul(i128::from(data.early_settlement_discount_bps))
                .ok_or(Error::Overflow)?
                .checked_div(i128::from(MAX_BPS))
                .ok_or(Error::Overflow)?;
            let discounted = data
                .face_value
                .checked_sub(discount)
                .ok_or(Error::Overflow)?
                .max(1); // floor at 1 stroop
                         // Emit the hook application event (only on first payment in the window
                         // to avoid redundant emissions on subsequent partial payments).
            if data.paid_amt == 0 {
                events::early_settlement_applied(
                    &env,
                    invoice_id.clone(),
                    data.early_settlement_discount_bps,
                    data.face_value,
                    discounted,
                );
            }
            discounted
        } else {
            data.face_value
        };

        // Remaining balance toward effective face value
        let remaining = effective_face_value
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
            return Err(Error::InvalidAmount);
        }

        // Category-specific fee rate overrides the platform default when the
        // admin has configured one via `set_category_fee` (see #377).
        let effective_fee_bps = storage::get_category_fee(&env, data.category)
            .map(|s| s.fee_bps)
            .unwrap_or(config.fee_bps);
        let fee_bps = i128::from(effective_fee_bps);
        // Fee is calculated on the payment amount (not face_value)
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;

        // Calculate penalty interest if payment is late but within grace period
        let mut penalty_interest: i128 = 0;
        if current_ts > data.due_dt && config.penalty_interest_bps > 0 {
            penalty_interest = investor_amount
                .checked_mul(i128::from(config.penalty_interest_bps))
                .ok_or(Error::Overflow)?
                .checked_div(i128::from(MAX_BPS))
                .ok_or(Error::Overflow)?;
        }
        // Penalty interest is added to platform fee
        let total_fee = platform_fee
            .checked_add(penalty_interest)
            .ok_or(Error::Overflow)?;
        let final_investor_amount = investor_amount
            .checked_sub(penalty_interest)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // 1. Pull payer's funds into escrow
        token.transfer(&payer, &contract, &amount);

        data.paid_amt = data.paid_amt.checked_add(amount).ok_or(Error::Overflow)?;

        // Settlement occurs when paid_amt reaches effective face value
        if data.paid_amt == effective_face_value {
            data.status = EscrowStatus::Settled;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);

        // Progress the installment milestone schedule (#450): mark every
        // milestone covered by the new cumulative paid amount (or all of them
        // when the escrow just reached terminal Settled).
        advance_installment_schedule(
            &env,
            &invoice_id,
            data.paid_amt,
            data.status == EscrowStatus::Settled,
        );

        let funder_addr = data.funder.clone().unwrap_or_else(|| data.seller.clone());

        if let Some(distributor) = config.payment_distributor.as_ref() {
            // The distributor must pay seller_amount (== amount) plus investor_amount + total_fee
            // (== amount), mirroring the direct path below which releases the payer's `amount` to the
            // seller in addition to paying the investor/admin out of escrow's held funding.
            let total_to_distributor = amount.checked_add(amount).ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);
            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    invoice_id.clone().into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.token, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.seller, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&funder_addr, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&config.admin, &env)
                    ]
                    .into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        data.paid_amt,
                        amount,
                        final_investor_amount,
                        config.fee_bps as i128,
                    ]
                    .into_val(&env),
                    (data.status as u32).into_val(&env)
                ],
            );
        } else {
            // 2. Platform fee + penalty interest to admin
            token.transfer(&contract, &config.admin, &total_fee);

            // 3. Pro-rata investor distribution with early repayment yield rebate
            if let Some(funder) = &data.funder {
                if data.funded_amt > 0 && final_investor_amount > 0 {
                    let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                    let rebate = calculate_early_repayment_rebate(
                        current_ts,
                        data.due_dt,
                        data.funded_dt,
                        investor_amount,
                    );
                    let net_investor_amount =
                        investor_amount.checked_sub(rebate).ok_or(Error::Overflow)?;
                    let pro_rata_share = net_investor_amount
                        .checked_mul(funder_amt)
                        .ok_or(Error::Overflow)?
                        .checked_div(data.funded_amt)
                        .ok_or(Error::Overflow)?;
                    if pro_rata_share > 0 {
                        token.transfer(&contract, funder, &pro_rata_share);
                    }
                }
            }
            // Seller receives the full payment amount
            token.transfer(&contract, &data.seller, &amount);
        }

        if platform_fee > 0 {
            events::fee_collected(&env, invoice_id.clone(), platform_fee, &config.admin);
        }

        if data.status == EscrowStatus::Settled {
            // Unlock invoice token transfers only when the invoice is completely settled.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

        if penalty_interest > 0 {
            events::penalty_interest_charged(&env, invoice_id.clone(), penalty_interest, total_fee);
        }
        events::payment_settled(
            &env,
            invoice_id.clone(),
            amount,
            total_fee,
            final_investor_amount,
        );
        if data.status == EscrowStatus::Settled {
            events::escrow_status_changed(
                &env,
                invoice_id,
                EscrowStatus::Settled,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Refund the investors if the invoice was not paid by due date. Anyone may call.
    /// Refunds are distributed pro-rata based on each investor's contribution.
    pub fn refund_escrow(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::RefundNotAllowed);
        }
        // Overdue eligibility: refund is only allowed once the grace window
        // (not just the bare due date) has fully lapsed -- see #375.
        let ledger_ts = env.ledger().timestamp();
        let overdue_at = data.due_dt.saturating_add(config.grace_period_seconds);
        if ledger_ts <= overdue_at {
            return Err(Error::EscrowNotOverdue);
        }
        events::grace_period_expired(
            &env,
            invoice_id.clone(),
            data.due_dt,
            config.grace_period_seconds,
        );

        // Refund the remaining collateral (purchase_price minus already released partial payments)
        let amount_to_refund = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Extract funder address before status mutation so it is available in both paths.
        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (data.status as u32).into_val(&env)
                    ],
                );
            } else {
                // Pro-rata refund to funders
                if let Some(funder) = &funder_opt {
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), funder);
                        let pro_rata_refund = amount_to_refund
                            .checked_mul(funder_amt)
                            .ok_or(Error::Overflow)?
                            .checked_div(data.funded_amt)
                            .ok_or(Error::Overflow)?;
                        if pro_rata_refund > 0 {
                            token.transfer(&contract, funder, &pro_rata_refund);
                        }
                    }
                }
            }
        }

        // Unlock invoice token transfers now that the invoice is refunded
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Refunded,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Raise a dispute on a `Funded` escrow, freezing it out of the normal
    /// settlement/refund paths until `resolve_dispute` runs. Either party
    /// (the seller or the debtor/buyer) may raise it.
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        invoice_id: Symbol,
        reason: Bytes,
    ) -> Result<(), Error> {
        caller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if caller != data.seller && caller != data.debtor {
            return Err(Error::Unauthorized);
        }
        if data.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }

        let raised_at = env.ledger().timestamp();
        data.status = EscrowStatus::Disputed;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        storage::set_dispute(
            &env,
            &invoice_id,
            &DisputeData {
                raiser: caller.clone(),
                reason,
                raised_at,
                resolved: false,
            },
        );

        events::dispute_raised(&env, invoice_id.clone(), &caller, raised_at);
        events::escrow_status_changed(&env, invoice_id, EscrowStatus::Disputed, raised_at);
        Ok(())
    }

    /// Admin-only: resolve a `Disputed` escrow in favour of `'seller'` (settles
    /// the escrow's held funds to the seller) or `'buyer'` (refunds them,
    /// pro-rata across funders, same as `refund_escrow`). If called after
    /// `Config::dispute_timeout_secs` has elapsed since the dispute was
    /// raised, the outcome is always the buyer-refund fallback regardless of
    /// `favour` -- an unresponsive admin cannot indefinitely strand a
    /// disputed escrow's funds.
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        favour: Symbol,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Disputed {
            return Err(Error::NotDisputed);
        }
        let mut dispute = storage::get_dispute(&env, &invoice_id).ok_or(Error::NotDisputed)?;
        if dispute.resolved {
            return Err(Error::AlreadyResolved);
        }

        let now = env.ledger().timestamp();
        let timed_out = now
            > dispute
                .raised_at
                .saturating_add(config.dispute_timeout_secs);
        // Past the timeout, the outcome is always refund-to-buyer regardless
        // of what `favour` was passed.
        let settle_to_seller = !timed_out && favour == Symbol::new(&env, "seller");

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        let held_amount = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        if settle_to_seller {
            if held_amount > 0 {
                token.transfer(&contract, &data.seller, &held_amount);
            }
            data.status = EscrowStatus::Settled;
        } else {
            if held_amount > 0 {
                if let Some(funder) = data.funder.clone() {
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), &funder);
                        let pro_rata_refund = held_amount
                            .checked_mul(funder_amt)
                            .ok_or(Error::Overflow)?
                            .checked_div(data.funded_amt)
                            .ok_or(Error::Overflow)?;
                        if pro_rata_refund > 0 {
                            token.transfer(&contract, &funder, &pro_rata_refund);
                        }
                    }
                }
            }
            data.status = EscrowStatus::Refunded;
        }
        storage::set_escrow(&env, invoice_id.clone(), &data);

        dispute.resolved = true;
        storage::set_dispute(&env, &invoice_id, &dispute);
        if settle_to_seller {
            advance_installment_schedule(&env, &invoice_id, data.paid_amt, true);
        }

        // Unlock invoice token transfers now that the dispute is resolved.
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        let resolved_favour = if settle_to_seller {
            Symbol::new(&env, "seller")
        } else {
            Symbol::new(&env, "buyer")
        };
        events::dispute_resolved(&env, invoice_id.clone(), resolved_favour, timed_out);
        events::escrow_status_changed(&env, invoice_id, data.status, now);
        Ok(())
    }

    /// Update platform fee (basis points). Admin only.
    pub fn update_platform_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        if new_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let old_fee_bps = config.fee_bps;
        config.fee_bps = new_fee_bps;
        storage::set_config(&env, &config);
        events::platform_fee_updated(&env, old_fee_bps, new_fee_bps);
        Ok(())
    }

    /// Set the payment distributor used for settlement/refund fan-out. Admin only.
    pub fn set_payment_distributor(env: Env, payment_distributor: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_distributor = config.payment_distributor.clone();
        config.payment_distributor = Some(payment_distributor.clone());
        storage::set_config(&env, &config);
        events::payment_distributor_updated(&env, old_distributor.is_some(), &payment_distributor);
        Ok(())
    }

    /// Set the referral contract called on each investment to accrue referral fees.
    /// Admin only. Pass the same address again to update, or a zero address is not
    /// accepted — use `clear_referral_contract` to remove the binding.
    pub fn set_referral_contract(env: Env, referral_contract: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        config.referral_contract = Some(referral_contract);
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Remove the referral contract binding so investments no longer trigger
    /// referral fee accrual. Admin only.
    pub fn clear_referral_contract(env: Env) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        config.referral_contract = None;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Toggle the emergency pause flag. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_paused = config.paused;
        config.paused = paused;
        storage::set_config(&env, &config);
        events::paused_updated(&env, old_paused, paused);
        Ok(())
    }

    /// View: return escrow data for an invoice, or Err(Error::EscrowNotFound) if not found.
    pub fn get_escrow(env: Env, invoice_id: Symbol) -> Result<EscrowData, Error> {
        storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return current config (admin and fee_bps).
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return escrow status for an invoice.
    pub fn get_escrow_status(env: Env, invoice_id: Symbol) -> Result<EscrowStatus, Error> {
        let data = storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(data.status)
    }

    /// View: return the current pause state.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.paused)
    }

    /// View: return the installment repayment milestone schedule for an
    /// invoice. Returns an empty vector when no schedule has been configured
    /// (or after `cleanup_escrow` reclaimed it).
    pub fn get_installment_schedule(
        env: Env,
        invoice_id: Symbol,
    ) -> Result<soroban_sdk::Vec<InstallmentMilestone>, Error> {
        storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        Ok(storage::get_installment_schedule(&env, &invoice_id)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env)))
    }

    /// View: return the next unsettled installment milestone, or `None` when
    /// no schedule exists or every milestone has been settled.
    pub fn get_next_installment(
        env: Env,
        invoice_id: Symbol,
    ) -> Result<Option<InstallmentMilestone>, Error> {
        storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        let schedule = storage::get_installment_schedule(&env, &invoice_id)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        for milestone in schedule.iter() {
            if !milestone.settled {
                return Ok(Some(milestone));
            }
        }
        Ok(None)
    }

    /// Admin-only: configure the emergency multi-sig admin set and threshold.
    pub fn set_emergency_config(
        env: Env,
        admin: Address,
        config: MultiSigConfig,
    ) -> Result<(), Error> {
        admin.require_auth();
        let stored_config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if stored_config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if config.threshold == 0 || config.threshold > config.admins.len() {
            return Err(Error::InvalidFeeBps); // reuse for invalid threshold
        }
        storage::set_emergency_config(&env, &config);
        Ok(())
    }

    /// Emergency multi-sig release: an admin approves releasing funds for an invoice.
    /// When the threshold is reached, funds are paid out to the seller and the escrow
    /// is marked as Settled.
    pub fn emergency_release(env: Env, caller: Address, invoice_id: Symbol) -> Result<bool, Error> {
        caller.require_auth();
        let config = storage::get_emergency_config(&env).ok_or(Error::EmergencyNotConfigured)?;

        // Verify caller is an emergency admin
        let mut is_admin = false;
        for admin in config.admins.iter() {
            if admin == caller {
                is_admin = true;
                break;
            }
        }
        if !is_admin {
            return Err(Error::NotEmergencyAdmin);
        }

        let mut approvals = storage::get_emergency_approvals(&env, &invoice_id);

        // Check for duplicate approval
        for addr in approvals.approvals.iter() {
            if addr == caller {
                return Err(Error::AlreadyApproved);
            }
        }

        approvals.approvals.push_back(caller.clone());
        storage::set_emergency_approvals(&env, &invoice_id, &approvals);

        if approvals.approvals.len() < config.threshold {
            return Ok(false);
        }

        // Threshold reached ? execute emergency release
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        if data.status == EscrowStatus::Settled
            || data.status == EscrowStatus::Refunded
            || data.status == EscrowStatus::Cancelled
        {
            return Err(Error::AlreadySettled);
        }

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        let remaining = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        // Pay remaining to seller
        if remaining > 0 {
            token.transfer(&contract, &data.seller, &remaining);
        }

        data.status = EscrowStatus::Settled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        advance_installment_schedule(&env, &invoice_id, data.paid_amt, true);

        events::escrow_status_changed(
            &env,
            invoice_id.clone(),
            EscrowStatus::Settled,
            env.ledger().timestamp(),
        );
        Ok(true)
    }

    /// Reclaim persistent storage for an escrow that has reached a terminal state
    /// (Settled, Refunded, or Cancelled). Callable only by the seller or the admin.
    /// The escrow and its per-funder contribution record are removed permanently;
    /// terminal-state escrows are never mutated again, so this is safe to prune.
    pub fn cleanup_escrow(env: Env, invoice_id: Symbol, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let data = storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if caller != data.seller && caller != config.admin {
            return Err(Error::Unauthorized);
        }
        match data.status {
            EscrowStatus::Settled | EscrowStatus::Refunded | EscrowStatus::Cancelled => {}
            _ => return Err(Error::EscrowNotSettled),
        }
        storage::remove_escrow_state(&env, invoice_id.clone(), &data.funders);
        storage::remove_installment_schedule(&env, &invoice_id);
        events::escrow_cleaned_up(&env, invoice_id);
        Ok(())
    }

    // ?? Position management: top_up / partial_refund / transfer_position / finalise_funding ??

    /// Create a funding invoice (BytesN<32> id) for the position management flows.
    ///
    /// Funding proceeds between `min_funding_threshold` and `max_funding_threshold`:
    /// * `invest` / `top_up` reject amounts that would push the total raised
    ///   above `max_funding_threshold` (partial investment up to the remaining
    ///   capacity is accepted);
    /// * `release_to_issuer` only succeeds once the total raised has reached
    ///   `min_funding_threshold`;
    /// * `ThresholdReached` is emitted the first time the minimum is crossed.
    ///
    /// Thresholds must satisfy `0 < min_funding_threshold <= funding_target <=
    /// max_funding_threshold`, so the target is always achievable within the cap.
    pub fn create_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        seller: Address,
        funding_target: i128,
        min_funding_threshold: i128,
        max_funding_threshold: i128,
        deadline_ledger: u32,
        min_investment: i128,
        per_investor_cap: Option<i128>,
        token: Address,
    ) -> Result<(), Error> {
        seller.require_auth();
        Self::create_invoice_core(
            &env,
            invoice_id,
            seller,
            funding_target,
            min_funding_threshold,
            max_funding_threshold,
            deadline_ledger,
            min_investment,
            per_investor_cap,
            token,
        )
    }

    /// Backwards-compatible `create_invoice` without explicit thresholds.
    /// Keeps the legacy behaviour: the invoice must be fully funded before
    /// release and can never exceed its target, i.e.
    /// `min_funding_threshold == max_funding_threshold == funding_target`.
    pub fn create_invoice_legacy(
        env: Env,
        invoice_id: BytesN<32>,
        seller: Address,
        funding_target: i128,
        deadline_ledger: u32,
        min_investment: i128,
        per_investor_cap: Option<i128>,
        token: Address,
    ) -> Result<(), Error> {
        seller.require_auth();
        Self::create_invoice_core(
            &env,
            invoice_id,
            seller,
            funding_target,
            funding_target,
            funding_target,
            deadline_ledger,
            min_investment,
            per_investor_cap,
            token,
        )
    }

    /// Shared validation + storage for `create_invoice` / `create_invoice_legacy`.
    fn create_invoice_core(
        env: &Env,
        invoice_id: BytesN<32>,
        seller: Address,
        funding_target: i128,
        min_funding_threshold: i128,
        max_funding_threshold: i128,
        deadline_ledger: u32,
        min_investment: i128,
        per_investor_cap: Option<i128>,
        token: Address,
    ) -> Result<(), Error> {
        if funding_target <= 0 {
            return Err(Error::InvalidAmount);
        }
        if min_funding_threshold <= 0
            || min_funding_threshold > funding_target
            || funding_target > max_funding_threshold
        {
            return Err(Error::InvalidFundingThreshold);
        }
        if min_investment < 0 {
            return Err(Error::InvalidAmount);
        }
        if storage::has_invoice(env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        let invoice = FundingInvoice {
            seller,
            funding_target,
            total_raised: 0,
            deadline_ledger,
            min_investment,
            per_investor_cap,
            status: InvoiceStatus::Open,
            token,
            min_funding_threshold,
            max_funding_threshold,
            threshold_reached: false,
        };
        storage::set_invoice(env, invoice_id.clone(), &invoice);
        events::funding_thresholds_set(
            env,
            &invoice_id,
            min_funding_threshold,
            max_funding_threshold,
        );
        Ok(())
    }

    /// Top up an existing investor position.
    /// Validates invoice is Open, caller has non-zero position, and cap not exceeded.
    pub fn top_up(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        additional_amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if additional_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Open {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current = storage::get_investor_position(&env, &invoice_id, &investor);
        if current == 0 {
            return Err(Error::NoPositionFound);
        }
        let new_total_position = current
            .checked_add(additional_amount)
            .ok_or(Error::Overflow)?;
        if let Some(cap) = invoice.per_investor_cap {
            if new_total_position > cap {
                return Err(Error::InvalidAmount);
            }
        }
        let new_total_raised = invoice
            .total_raised
            .checked_add(additional_amount)
            .ok_or(Error::Overflow)?;
        if new_total_raised > invoice.max_funding_threshold {
            return Err(Error::MaxFundingThresholdExceeded);
        }
        // Transfer additional_amount from investor to contract
        let token_client = token::Client::new(&env, &invoice.token);
        token_client.transfer(
            &investor,
            env.current_contract_address(),
            &additional_amount,
        );
        storage::set_investor_position(&env, &invoice_id, &investor, new_total_position);
        invoice.total_raised = new_total_raised;
        if !invoice.threshold_reached && new_total_raised >= invoice.min_funding_threshold {
            invoice.threshold_reached = true;
            events::threshold_reached(
                &env,
                &invoice_id,
                invoice.min_funding_threshold,
                new_total_raised,
            );
        }
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
        events::investment_topped_up(
            &env,
            &investor,
            invoice_id,
            additional_amount,
            new_total_position,
        );
        Ok(())
    }

    // ---------- Invoice Registration, Investment, Refund, Settlement & TTL Refresh ----------

    /// Register invoice metadata and funding parameters on-chain. Callable only by admin.
    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        face_value: i128,
        funding_target: i128,
        yield_bps: u32,
        deadline_ledger: u32,
        document_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        if face_value <= 0 || funding_target <= 0 {
            return Err(Error::InvalidAmount);
        }
        if !(1..=2500).contains(&yield_bps) {
            return Err(Error::InvalidYield);
        }
        if storage::has_invoice_record(&env, &invoice_id) {
            return Err(Error::InvoiceAlreadyExists);
        }

        let data = InvoiceData {
            invoice_id: invoice_id.clone(),
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
            total_raised: 0,
            status: EscrowStatus::Created,
            investors: soroban_sdk::Vec::new(&env),
            document_hash,
        };

        storage::set_invoice_record(&env, &invoice_id, &data);
        storage::set_investor_count(&env, &invoice_id, 0);
        events::invoice_registered(
            &env,
            &invoice_id,
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
        );
        Ok(())
    }

    /// Cancel a registered invoice before funding completes. Open positions
    /// remain refundable through `refund`; funded invoices cannot be cancelled.
    pub fn cancel_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created || record.total_raised >= record.funding_target {
            return Err(Error::InvalidInvoiceStatus);
        }
        record.status = EscrowStatus::Cancelled;
        storage::set_invoice_record(&env, &invoice_id, &record);
        events::invoice_cancelled(&env, &invoice_id, &config.admin);
        Ok(())
    }

    /// Admin-only: extend the funding deadline for an open invoice.
    pub fn extend_deadline(
        env: Env,
        invoice_id: BytesN<32>,
        new_deadline_ledger: u32,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let mut invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Open {
            return Err(Error::InvalidInvoiceStatus);
        }

        let old_deadline_ledger = invoice.deadline_ledger;
        if new_deadline_ledger <= old_deadline_ledger {
            return Err(Error::DeadlineNotExtended);
        }

        invoice.deadline_ledger = new_deadline_ledger;
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
        events::deadline_extended(&env, &invoice_id, old_deadline_ledger, new_deadline_ledger);
        Ok(())
    }
    /// Invest in a registered invoice or funding invoice.
    pub fn invest(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        Self::invest_core(env, invoice_id, investor, amount, None)
    }

    /// Admin-only: set the FX oracle contract used to normalise investments
    /// made in a foreign currency. The oracle must expose
    /// `get_price(currency: Symbol) -> PriceData` returning the rate of one
    /// unit of `currency` in base currency (scaled by `decimals`).
    pub fn set_fx_oracle(env: Env, admin: Address, oracle: Address) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &oracle)?;
        storage::set_fx_oracle(&env, &oracle);
        Ok(())
    }

    /// Admin-only: list `currency` (e.g. USDC, EURC, NGNC) as an accepted
    /// investment currency paid in `token`.
    pub fn set_supported_currency(
        env: Env,
        admin: Address,
        currency: Symbol,
        token: Address,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &token)?;
        storage::set_currency_token(&env, &currency, Some(&token));
        Ok(())
    }

    /// Admin-only: remove `currency` from the accepted investment currencies.
    pub fn remove_supported_currency(
        env: Env,
        admin: Address,
        currency: Symbol,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::set_currency_token(&env, &currency, None);
        Ok(())
    }

    /// View: the payment token of a supported investment currency, if listed.
    pub fn get_supported_currency(env: Env, currency: Symbol) -> Option<Address> {
        storage::get_currency_token(&env, &currency)
    }

    /// Invest `amount` of `currency` in a funding invoice. The amount is
    /// converted to base currency at the FX oracle rate fetched now, and the
    /// normalised base amount is what the investor position, the invoice
    /// total and every pro-rata share are computed from. Returns the base
    /// amount credited.
    pub fn invest_with_currency(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        amount: i128,
        currency: Symbol,
    ) -> Result<i128, Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let pay_token =
            storage::get_currency_token(&env, &currency).ok_or(Error::UnsupportedCurrency)?;
        let oracle = storage::get_fx_oracle(&env).ok_or(Error::FxOracleNotSet)?;
        if !storage::has_invoice(&env, invoice_id.clone()) {
            return Err(Error::EscrowNotFound);
        }

        let price: PriceData = env.invoke_contract(
            &oracle,
            &Symbol::new(&env, "get_price"),
            (currency.clone(),).into_val(&env),
        );
        let now = env.ledger().timestamp();
        if now.saturating_sub(price.timestamp) > OraclePriceAdapter::DEFAULT_MAX_STALENESS_SECS {
            return Err(Error::StalePrice);
        }
        if price.price <= 0 {
            return Err(Error::InvalidAmount);
        }
        let scale = 10_i128
            .checked_pow(price.decimals)
            .ok_or(Error::Overflow)?;
        let base_amount = amount
            .checked_mul(price.price)
            .ok_or(Error::Overflow)?
            / scale;
        if base_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        Self::invest_core(
            env.clone(),
            invoice_id.clone(),
            investor.clone(),
            base_amount,
            Some((pay_token, amount)),
        )?;

        storage::add_investment_record(
            &env,
            &invoice_id,
            &investor,
            &InvestmentRecord {
                currency: currency.clone(),
                amount,
                rate: price.price,
                decimals: price.decimals,
                base_amount,
                timestamp: now,
            },
        );
        events::currency_invested(&env, &invoice_id, &investor, &currency, amount, base_amount);
        Ok(base_amount)
    }

    /// View: multi-currency investment records for an investor on an invoice.
    pub fn get_investment_records(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Vec<InvestmentRecord> {
        storage::get_investment_records(&env, &invoice_id, &investor)
    }

    /// Shared investment path. `amount` is the base-currency amount credited
    /// to the position; `foreign` is the (token, amount) actually paid when the
    /// investment is made in another currency (funding invoices only).
    fn invest_core(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        amount: i128,
        foreign: Option<(Address, i128)>,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if let Some(mut invoice) = storage::get_invoice(&env, invoice_id.clone()) {
            if invoice.status != InvoiceStatus::Open {
                return Err(Error::InvalidInvoiceStatus);
            }
            if amount < invoice.min_investment {
                return Err(Error::BelowMinimumInvestment);
            }
            if let Some(cap) = invoice.per_investor_cap {
                if amount > cap {
                    return Err(Error::InvalidAmount);
                }
            }
            let new_total = invoice
                .total_raised
                .checked_add(amount)
                .ok_or(Error::Overflow)?;
            if new_total > invoice.max_funding_threshold {
                return Err(Error::MaxFundingThresholdExceeded);
            }
            let current = storage::get_investor_position(&env, &invoice_id, &investor);
            let new_pos = current.checked_add(amount).ok_or(Error::Overflow)?;
            if let Some(cap) = invoice.per_investor_cap {
                if new_pos > cap {
                    return Err(Error::InvalidAmount);
                }
            }
            let (pay_token, pay_amount) = match &foreign {
                Some((pay_token, paid)) => (pay_token.clone(), *paid),
                None => (invoice.token.clone(), amount),
            };
            let token_client = token::Client::new(&env, &pay_token);
            token_client.transfer(&investor, env.current_contract_address(), &pay_amount);
            storage::set_investor_position(&env, &invoice_id, &investor, new_pos);
            invoice.total_raised = new_total;
            if !invoice.threshold_reached && new_total >= invoice.min_funding_threshold {
                invoice.threshold_reached = true;
                events::threshold_reached(
                    &env,
                    &invoice_id,
                    invoice.min_funding_threshold,
                    new_total,
                );
            }
            storage::set_invoice(&env, invoice_id, &invoice);
            return Ok(());
        }

        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current_ledger = env.ledger().sequence();
        if current_ledger > record.deadline_ledger {
            return Err(Error::FundingDeadlineNotPassed);
        }

        let new_raised = record
            .total_raised
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        if new_raised > record.funding_target {
            return Err(Error::InvalidAmount);
        }

        let mut already_in = false;
        for existing in record.investors.iter() {
            if existing == investor {
                already_in = true;
                break;
            }
        }
        if !already_in {
            let investor_count = storage::get_investor_count(&env, &invoice_id);
            if investor_count >= storage::get_max_investors(&env) {
                return Err(Error::MaxInvestorsReached);
            }
            storage::set_investor_count(
                &env,
                &invoice_id,
                investor_count.checked_add(1).ok_or(Error::Overflow)?,
            );
            record.investors.push_back(investor.clone());
        }

        let current_pos = storage::get_investor_position(&env, &invoice_id, &investor);
        let new_pos = current_pos.checked_add(amount).ok_or(Error::Overflow)?;
        storage::set_investor_position(&env, &invoice_id, &investor, new_pos);

        record.total_raised = new_raised;
        if record.total_raised == record.funding_target {
            record.status = EscrowStatus::Funded;
        }

        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// Partially refund an investor's position before deadline.
    pub fn partial_refund(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Open {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current_ledger = env.ledger().sequence();
        if current_ledger >= invoice.deadline_ledger {
            return Err(Error::InvalidInvoiceStatus);
        }
        let current = storage::get_investor_position(&env, &invoice_id, &investor);
        if current == 0 {
            return Err(Error::NoPositionFound);
        }
        if amount > current {
            return Err(Error::InvalidAmount);
        }
        let remaining = current.checked_sub(amount).ok_or(Error::Overflow)?;
        if remaining != 0 && remaining < invoice.min_investment {
            return Err(Error::BelowMinimumInvestment);
        }
        let new_total_raised = invoice
            .total_raised
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        // Transfer back to caller
        let token_client = token::Client::new(&env, &invoice.token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);
        storage::set_investor_position(&env, &invoice_id, &investor, remaining);
        invoice.total_raised = new_total_raised;
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
        events::investment_partially_refunded(&env, &investor, invoice_id, amount, remaining);
        Ok(())
    }

    /// Transfer a funded position from seller to buyer for an agreed price.
    pub fn transfer_position(
        env: Env,
        from: Address,
        invoice_id: BytesN<32>,
        to: Address,
        price: i128,
    ) -> Result<(), Error> {
        from.require_auth();
        if price < 0 {
            return Err(Error::InvalidAmount);
        }
        let invoice =
            storage::get_invoice(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if invoice.status != InvoiceStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        let position = storage::get_investor_position(&env, &invoice_id, &from);
        if position == 0 {
            return Err(Error::NoPositionFound);
        }
        to.require_auth();
        buy_limit::record_purchase(&env, &invoice_id, &to, position)?;
        if price > 0 {
            let token_client = token::Client::new(&env, &invoice.token);
            token_client.transfer(&to, &from, &price);
        }
        let buyer_existing = storage::get_investor_position(&env, &invoice_id, &to);
        let new_buyer_pos = buyer_existing
            .checked_add(position)
            .ok_or(Error::Overflow)?;
        if let Some(cap) = invoice.per_investor_cap {
            if new_buyer_pos > cap {
                return Err(Error::InvalidAmount);
            }
        }
        storage::set_investor_position(&env, &invoice_id, &from, 0);
        storage::set_investor_position(&env, &invoice_id, &to, new_buyer_pos);
        events::position_transferred(&env, &from, &to, invoice_id, position, price);
        Ok(())
    }

    /// Release funding proceeds to the issuer (seller) once the minimum funding
    /// threshold has been reached. Admin-only.
    ///
    /// For a `FundingInvoice`, requires `total_raised >= min_funding_threshold`,
    /// transitions the invoice Open -> Funded, and transfers the raised amount
    /// to the seller. For a registered `InvoiceData` invoice the minimum
    /// threshold defaults to its funding target.
    pub fn release_to_issuer(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        if let Some(mut invoice) = storage::get_invoice(&env, invoice_id.clone()) {
            if invoice.status != InvoiceStatus::Open {
                return Err(Error::InvalidInvoiceStatus);
            }
            if invoice.total_raised < invoice.min_funding_threshold {
                return Err(Error::FundingThresholdNotReached);
            }
            invoice.status = InvoiceStatus::Funded;
            storage::set_invoice(&env, invoice_id.clone(), &invoice);
            if invoice.total_raised > 0 {
                let token_client = token::Client::new(&env, &invoice.token);
                token_client.transfer(
                    &env.current_contract_address(),
                    &invoice.seller,
                    &invoice.total_raised,
                );
            }
            events::funding_finalised(&env, invoice_id, invoice.total_raised, &invoice.seller);
            return Ok(());
        }

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created && record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        if record.total_raised < record.funding_target {
            return Err(Error::FundingThresholdNotReached);
        }
        record.status = EscrowStatus::Funded;
        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// Finalise funding: legacy alias for `release_to_issuer` on position
    /// invoices, and Created/Funded -> Funded for registered invoices.
    pub fn finalise_funding(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        if storage::get_invoice(&env, invoice_id.clone()).is_some() {
            return Self::release_to_issuer(env, invoice_id);
        }

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Created && record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        record.status = EscrowStatus::Funded;
        storage::set_invoice_record(&env, &invoice_id, &record);
        Ok(())
    }

    /// View: funding progress for an invoice (registered or position invoice).
    ///
    /// Returns the total raised, the configured minimum/maximum funding
    /// thresholds, and the percentage of the funding target filled in basis
    /// points (10_000 = 100%). Registered invoices use their funding target as
    /// both the minimum and maximum threshold.
    pub fn get_funding_status(env: Env, invoice_id: BytesN<32>) -> Result<FundingStatus, Error> {
        if let Some(invoice) = storage::get_invoice(&env, invoice_id.clone()) {
            return Ok(FundingStatus {
                raised: invoice.total_raised,
                min_threshold: invoice.min_funding_threshold,
                max_threshold: invoice.max_funding_threshold,
                percent_filled_bps: percent_filled_bps(invoice.total_raised, invoice.funding_target),
            });
        }
        let record = storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(FundingStatus {
            raised: record.total_raised,
            min_threshold: record.funding_target,
            max_threshold: record.funding_target,
            percent_filled_bps: percent_filled_bps(record.total_raised, record.funding_target),
        })
    }

    /// Refund an investor's committed position if deadline passed without reaching target.
    pub fn refund(env: Env, invoice_id: BytesN<32>, investor: Address) -> Result<(), Error> {
        investor.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut record =
            storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;

        if record.status == EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }

        if record.status != EscrowStatus::Cancelled
            && env.ledger().sequence() <= record.deadline_ledger
        {
            return Err(Error::FundingDeadlineNotPassed);
        }

        let committed = storage::get_investor_position(&env, &invoice_id, &investor);
        if committed <= 0 {
            return Err(Error::NoPositionFound);
        }

        storage::remove_investor_position(&env, &invoice_id, &investor);

        record.total_raised = record
            .total_raised
            .checked_sub(committed)
            .ok_or(Error::Overflow)?;

        storage::set_invoice_record(&env, &invoice_id, &record);
        events::investment_refunded(&env, &investor, &invoice_id, committed);
        Ok(())
    }

    /// Backward-compatible proposal entry point. Settlement now requires a second
    /// configured administrator to call `approve_settlement` before execution.
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        Self::propose_settlement_core(&env, &config.admin, &invoice_id, repayment_amount)
    }

    /// Propose a settlement. Only an administrator in the configured
    /// multi-signature set may propose; a different administrator must approve.
    pub fn propose_settlement(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::propose_settlement_core(&env, &admin, &invoice_id, repayment_amount)
    }

    fn propose_settlement_core(
        env: &Env,
        admin: &Address,
        invoice_id: &BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), Error> {
        let record = storage::get_invoice_record(env, invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        if repayment_amount < record.total_raised {
            return Err(Error::InsufficientRepayment);
        }
        let multisig = storage::get_emergency_config(env).ok_or(Error::EmergencyNotConfigured)?;
        if multisig.threshold != 2 || !multisig.admins.iter().any(|member| member == *admin) {
            return Err(Error::NotEmergencyAdmin);
        }
        if storage::get_settlement_proposal(env, invoice_id).is_some() {
            return Err(Error::InvalidInvoiceStatus);
        }
        storage::set_settlement_proposal(
            env,
            invoice_id,
            &SettlementProposal {
                proposer: admin.clone(),
                repayment_amount,
            },
        );
        events::settlement_proposed(env, invoice_id, admin, repayment_amount);
        Ok(())
    }

    /// Approve and execute a proposed settlement. The proposer cannot approve
    /// their own proposal, and the approval set is always exactly two admins.
    pub fn approve_settlement(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        admin.require_auth();
        let multisig = storage::get_emergency_config(&env).ok_or(Error::EmergencyNotConfigured)?;
        if multisig.threshold != 2 || !multisig.admins.iter().any(|member| member == admin) {
            return Err(Error::NotEmergencyAdmin);
        }
        let proposal = storage::get_settlement_proposal(&env, &invoice_id)
            .ok_or(Error::SettlementNotProposed)?;
        if proposal.proposer == admin {
            return Err(Error::SameAdminApproval);
        }
        Self::execute_settlement(&env, &invoice_id, proposal.repayment_amount)?;
        storage::remove_settlement_proposal(&env, &invoice_id);
        events::settlement_approved(&env, &invoice_id, &admin, proposal.repayment_amount);
        Ok(())
    }

    fn execute_settlement(
        env: &Env,
        invoice_id: &BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), Error> {
        let mut record =
            storage::get_invoice_record(env, invoice_id).ok_or(Error::EscrowNotFound)?;
        if record.status != EscrowStatus::Funded {
            return Err(Error::InvalidInvoiceStatus);
        }
        if repayment_amount < record.total_raised {
            return Err(Error::InsufficientRepayment);
        }
        let mut total_payouts: i128 = 0;
        for investor in record.investors.iter() {
            let committed = storage::get_investor_position(env, invoice_id, &investor);
            if committed > 0 {
                let payout = committed
                    .checked_mul(repayment_amount)
                    .ok_or(Error::Overflow)?
                    .checked_div(record.total_raised)
                    .ok_or(Error::Overflow)?;
                let yield_earned = payout.saturating_sub(committed);
                total_payouts = total_payouts.checked_add(payout).ok_or(Error::Overflow)?;
                events::settlement_paid(env, &investor, invoice_id, payout, yield_earned);
            }
        }
        let _dust = repayment_amount
            .checked_sub(total_payouts)
            .ok_or(Error::Overflow)?;
        record.status = EscrowStatus::Settled;
        storage::set_invoice_record(env, invoice_id, &record);
        Ok(())
    }

    /// Admin-only: refresh TTL for invoice record and all investor position entries.
    pub fn refresh_all_ttls(env: Env, invoice_id: BytesN<32>) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let record = storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)?;
        for investor in record.investors.iter() {
            let _ = storage::get_investor_position(&env, &invoice_id, &investor);
        }
        Ok(())
    }

    /// View: get funding invoice (BytesN<32>).
    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<FundingInvoice, Error> {
        storage::get_invoice(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return registered invoice data.
    pub fn get_invoice_record(env: Env, invoice_id: BytesN<32>) -> Result<InvoiceData, Error> {
        storage::get_invoice_record(&env, &invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: number of unique investors admitted to the registered invoice.
    pub fn get_invoice_investor_count(env: Env, invoice_id: BytesN<32>) -> u32 {
        storage::get_investor_count(&env, &invoice_id)
    }

    /// View: return investor position amount for an invoice.
    pub fn get_investor_position(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<i128, Error> {
        Ok(storage::get_investor_position(&env, &invoice_id, &investor))
    }

    /// Paginated query to retrieve multiple escrows by sequential creation order.
    pub fn get_escrows(
        env: Env,
        start: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<EscrowData>, Error> {
        const MAX_PAGE_SIZE: u32 = 100;

        if limit == 0 {
            return Err(Error::InvalidLimit);
        }
        if limit > MAX_PAGE_SIZE {
            return Err(Error::LimitExceeded);
        }

        let total_count = storage::get_escrow_count(&env);

        if start >= total_count {
            return Ok(soroban_sdk::Vec::new(&env));
        }

        let end = core::cmp::min(start + limit, total_count);
        let mut results = soroban_sdk::Vec::new(&env);

        for index in start..end {
            if let Some(invoice_id) = storage::get_escrow_id_by_index(&env, index) {
                if let Some(escrow_data) = storage::get_escrow(&env, invoice_id) {
                    results.push_back(escrow_data);
                }
            }
        }

        Ok(results)
    }

    // ── Issue #443: Two-step timelock controller for admin parameter updates ──

    /// Default timelock period: 24 hours (86400 seconds).
    const DEFAULT_TIMELOCK_SECS: u64 = 86_400;

    /// Propose a critical admin parameter change. The change cannot be executed
    /// until `timelock_secs` seconds have elapsed, giving participants time to
    /// react (e.g. withdraw before a fee increase takes effect).
    pub fn propose_param_change(
        env: Env,
        admin: Address,
        param: types::ParamType,
        new_value: i128,
        timelock_secs: Option<u64>,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let delay = timelock_secs.unwrap_or(Self::DEFAULT_TIMELOCK_SECS);
        if delay == 0 {
            return Err(Error::InvalidAmount);
        }

        let proposal = types::PendingParamChange {
            param,
            new_value,
            proposed_at: env.ledger().timestamp(),
            timelock_secs: delay,
            proposer: admin,
        };

        storage::set_pending_param_change(&env, &proposal);
        events::param_change_proposed(&env, &proposal);
        Ok(())
    }

    /// Execute a previously proposed parameter change after the timelock has
    /// expired. Anyone may call this (permissionless execution) — the security
    /// property is the delay, not the caller identity.
    pub fn execute_param_change(env: Env) -> Result<(), Error> {
        let proposal =
            storage::get_pending_param_change(&env).ok_or(Error::InvalidEarlySettlement)?;

        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(proposal.proposed_at);
        if elapsed < proposal.timelock_secs {
            return Err(Error::InvalidDuration);
        }

        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;

        match proposal.param {
            types::ParamType::FeeBps => {
                let new_bps =
                    u32::try_from(proposal.new_value).map_err(|_| Error::InvalidFeeBps)?;
                if new_bps > MAX_BPS {
                    return Err(Error::InvalidFeeBps);
                }
                let old = config.fee_bps;
                config.fee_bps = new_bps;
                storage::set_config(&env, &config);
                events::platform_fee_updated(&env, old, new_bps);
            }
            types::ParamType::GracePeriod => {
                let new_secs = u64::try_from(proposal.new_value).map_err(|_| Error::Overflow)?;
                let old = config.grace_period_seconds;
                config.grace_period_seconds = new_secs;
                storage::set_config(&env, &config);
                events::grace_period_updated(&env, old, new_secs);
            }
            types::ParamType::MinInvestment => {
                if proposal.new_value < 0 {
                    return Err(Error::InvalidAmount);
                }
                config.min_investment = proposal.new_value;
                storage::set_config(&env, &config);
            }
            types::ParamType::DisputeTimeout => {
                let new_secs = u64::try_from(proposal.new_value).map_err(|_| Error::Overflow)?;
                config.dispute_timeout_secs = new_secs;
                storage::set_config(&env, &config);
            }
            types::ParamType::PenaltyInterestBps => {
                let new_bps =
                    u32::try_from(proposal.new_value).map_err(|_| Error::InvalidFeeBps)?;
                if new_bps > MAX_BPS {
                    return Err(Error::InvalidFeeBps);
                }
                config.penalty_interest_bps = new_bps;
                storage::set_config(&env, &config);
            }
        }

        storage::clear_pending_param_change(&env);
        events::param_change_executed(&env, &proposal);
        Ok(())
    }

    /// View: return the pending parameter change, if any.
    pub fn get_pending_param_change(env: Env) -> Option<types::PendingParamChange> {
        storage::get_pending_param_change(&env)
    }

    // ── Issue #469: Batch TTL extension for off-chain keeper bots ─────────────

    /// Extend the TTL of multiple persistent storage keys in a single call.
    /// Off-chain keeper bots can use this to keep active escrows alive without
    /// issuing separate transactions per key. Only extends keys that exist;
    /// missing keys are silently skipped.
    pub fn batch_extend_ttl(env: Env, keys: Vec<types::StorageKey>) {
        storage::batch_extend_ttl(&env, &keys);
    }

    // ── Issue #468: Storage footprint compaction for completed invoices ───────

    /// Remove all persistent storage entries associated with a completed
    /// invoice to reclaim storage footprint. Only callable by admin and only
    /// after the invoice has reached a terminal state (Settled, Refunded, or
    /// Cancelled).
    ///
    /// Returns the number of storage keys removed.
    pub fn compact_invoice_storage(
        env: Env,
        admin: Address,
        inv_id: Symbol,
        funder_addresses: Vec<Address>,
    ) -> Result<u32, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        // Verify invoice is in a terminal state
        let escrow = storage::get_escrow(&env, inv_id.clone()).ok_or(Error::EscrowNotFound)?;
        match escrow.status {
            types::EscrowStatus::Settled
            | types::EscrowStatus::Refunded
            | types::EscrowStatus::Cancelled => {}
            _ => return Err(Error::InvalidInvoiceStatus),
        }

        let removed = storage::compact_invoice_storage(&env, &inv_id, &funder_addresses);
        events::escrow_cleaned_up(&env, inv_id);
        Ok(removed)
    }
}

#[cfg(test)]
mod benchmarks;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
#[cfg(test)]
mod test_buy_limit;
#[cfg(test)]
mod test_multi_currency;

// Modules added for batch resolution of #479, #480, #481
pub mod insurance_pool;
pub mod oracle_adapter;
pub mod earnest_deposit;

#[cfg(test)]
mod test_batch_features;

pub use insurance_pool::{InsuranceConfig, InsurancePool, InsuranceStorageKey};
pub use oracle_adapter::{CurrencyOracleConfig, OracleError, OraclePriceAdapter, PriceData};
pub use earnest_deposit::{EarnestDeposit, EarnestDepositConfig, EarnestError, EarnestStatus};
