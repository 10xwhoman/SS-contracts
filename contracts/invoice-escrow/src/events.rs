#![allow(deprecated)]
//! Event definitions for state changes (escrow_created, escrow_funded, payment_settled).
//!
//! #471 — All events include the contract address as the first topic for
//! indexer standardization. Off-chain indexers (Zephyr, Mercury) can filter
//! by contract address without inspecting the event body.

use soroban_sdk::{Address, BytesN, Env, Symbol, Val};

use crate::types::EscrowStatus;

/// Helper to emit a standardized event with the contract address as the first
/// topic. This ensures all events follow a consistent schema:
///   topics: [contract_address, event_name, ...]
///   data: <payload>
#[allow(dead_code)]
fn emit_event(env: &Env, event_name: &str, topics: soroban_sdk::Vec<Val>, data: Val) {
    let contract = env.current_contract_address();
    let mut full_topics = soroban_sdk::Vec::new(env);
    full_topics.push_back(contract.to_val());
    full_topics.push_back(Symbol::new(env, event_name).to_val());
    for t in topics.iter() {
        full_topics.push_back(t);
    }
    env.events().publish(full_topics, data);
}

/// Publish a lifecycle transition event carrying the new status and ledger
/// timestamp, in addition to the narrower per-action events below. Lets
/// off-chain indexers reconstruct full escrow lifecycle history/metadata
/// from a single event stream instead of correlating five separate events.
pub fn escrow_status_changed(env: &Env, inv_id: Symbol, status: EscrowStatus, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "escrow_status_changed"),),
        (inv_id, status as u32, timestamp),
    );
}

pub fn escrow_created(
    env: &Env,
    inv_id: Symbol,
    seller: &Address,
    debtor: &Address,
    face_value: i128,
    purchase_price: i128,
    due_dt: u64,
    token: &Address,
    inv_token: &Address,
    commitment: &soroban_sdk::BytesN<32>,
    funding_milestone: Option<i128>,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_created"),),
        (
            inv_id.clone(),
            seller,
            debtor,
            face_value,
            purchase_price,
            due_dt,
            token,
            inv_token,
            commitment,
            funding_milestone,
        ),
    );
}

/// Publish escrow_funded event with partial funding info.
pub fn escrow_funded(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
    amount: i128,
    funded_amt: i128,
    purchase_price: i128,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_funded"),),
        (inv_id, funder, amount, funded_amt, purchase_price),
    );
}

/// Publish penalty_interest_charged event when late payment incurs additional interest.
pub fn penalty_interest_charged(env: &Env, inv_id: Symbol, penalty_amount: i128, total_fee: i128) {
    env.events().publish(
        (Symbol::new(env, "penalty_interest_charged"),),
        (inv_id, penalty_amount, total_fee),
    );
}

/// Publish payment_settled event (amount, platform_fee, investor_amount).
pub fn payment_settled(
    env: &Env,
    inv_id: Symbol,
    amount: i128,
    platform_fee: i128,
    investor_amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "payment_settled"),),
        (inv_id, amount, platform_fee, investor_amount),
    );
}

/// Publish refund event.
pub fn escrow_refunded(env: &Env, inv_id: Symbol, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "escrow_refunded"),), (inv_id, amount));
}

/// Publish escrow_cancelled event (invoice_id, seller).
pub fn escrow_cancelled(env: &Env, inv_id: Symbol, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "escrow_cancelled"),), (inv_id, seller));
}

/// Publish escrow_funded event for a signed off-chain approval, including the consumed nonce.
pub fn escrow_funded_signed(env: &Env, inv_id: Symbol, buyer: &Address, amount: i128, nonce: u64) {
    env.events().publish(
        (Symbol::new(env, "escrow_fund_sig"),),
        (inv_id, buyer, amount, nonce),
    );
}

/// Publish escrow_cleaned_up event once a terminal escrow's storage has been reclaimed.
pub fn escrow_cleaned_up(env: &Env, inv_id: Symbol) {
    env.events()
        .publish((Symbol::new(env, "escrow_cleaned"),), inv_id);
}

/// Publish platform fee update event with old and new basis points.
pub fn platform_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (Symbol::new(env, "platform_fee_updated"),),
        (old_fee_bps, new_fee_bps),
    );
}

/// Publish payment distributor update event with previous and new distributor addresses.
pub fn payment_distributor_updated(
    env: &Env,
    had_previous_distributor: bool,
    new_distributor: &Address,
) {
    env.events().publish(
        (
            Symbol::new(env, "distributor_updated"),
            new_distributor.clone(),
        ),
        had_previous_distributor,
    );
}

/// Publish paused state updates.
pub fn paused_updated(env: &Env, old_paused: bool, new_paused: bool) {
    env.events().publish(
        (Symbol::new(env, "paused_updated"),),
        (old_paused, new_paused),
    );
}

/// Emitted when an early-settlement discount hook is applied during `record_payment`.
/// `original_face` is the unmodified face value; `effective_face` is the discounted value
/// that will be used as the settlement target.
pub fn early_settlement_applied(
    env: &Env,
    inv_id: Symbol,
    discount_bps: u32,
    original_face: i128,
    effective_face: i128,
) {
    env.events().publish(
        (Symbol::new(env, "early_settlement_applied"),),
        (inv_id, discount_bps, original_face, effective_face),
    );
}

/// Investment topped up event.
pub fn investment_topped_up(
    env: &Env,
    investor: &Address,
    invoice_id: BytesN<32>,
    additional_amount: i128,
    new_total_position: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "investment_topped_up"),
            investor.clone(),
            invoice_id.clone(),
        ),
        (additional_amount, new_total_position),
    );
}

/// Investment partially refunded event.
pub fn investment_partially_refunded(
    env: &Env,
    investor: &Address,
    invoice_id: BytesN<32>,
    amount_refunded: i128,
    remaining_position: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "investment_partially_refunded"),
            investor.clone(),
            invoice_id.clone(),
        ),
        (amount_refunded, remaining_position),
    );
}

/// Position transferred event.
pub fn position_transferred(
    env: &Env,
    from: &Address,
    to: &Address,
    invoice_id: BytesN<32>,
    position_amount: i128,
    price: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "position_transferred"),
            from.clone(),
            to.clone(),
            invoice_id.clone(),
        ),
        (position_amount, price),
    );
}

/// Publish funding finalised event.
pub fn funding_finalised(env: &Env, invoice_id: BytesN<32>, total_raised: i128, seller: &Address) {
    env.events().publish(
        (Symbol::new(env, "funding_finalised"), invoice_id.clone()),
        (total_raised, seller.clone()),
    );
}

/// Publish `ThresholdReached` the first time total raised crosses the invoice's
/// minimum funding threshold. Emitted exactly once per invoice.
pub fn threshold_reached(
    env: &Env,
    invoice_id: &BytesN<32>,
    min_threshold: i128,
    total_raised: i128,
) {
    env.events().publish(
        (Symbol::new(env, "ThresholdReached"), invoice_id.clone()),
        (min_threshold, total_raised),
    );
}

/// Publish the per-invoice funding thresholds when an invoice is created.
pub fn funding_thresholds_set(
    env: &Env,
    invoice_id: &BytesN<32>,
    min_threshold: i128,
    max_threshold: i128,
) {
    env.events().publish(
        (Symbol::new(env, "funding_thresholds_set"), invoice_id.clone()),
        (min_threshold, max_threshold),
    );
}

/// Publish invoice_registered event with all parameters.
pub fn invoice_registered(
    env: &Env,
    invoice_id: &soroban_sdk::BytesN<32>,
    face_value: i128,
    funding_target: i128,
    yield_bps: u32,
    deadline_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "invoice_registered"),),
        (
            invoice_id.clone(),
            face_value,
            funding_target,
            yield_bps,
            deadline_ledger,
        ),
    );
}

/// Publish deadline_extended event with old and new ledger deadlines.
pub fn deadline_extended(
    env: &Env,
    invoice_id: &soroban_sdk::BytesN<32>,
    old_deadline_ledger: u32,
    new_deadline_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "deadline_extended"),),
        (invoice_id.clone(), old_deadline_ledger, new_deadline_ledger),
    );
}
/// Publish investment_refunded event.
pub fn investment_refunded(
    env: &Env,
    investor: &Address,
    invoice_id: &soroban_sdk::BytesN<32>,
    amount_refunded: i128,
) {
    env.events().publish(
        (Symbol::new(env, "investment_refunded"),),
        (investor.clone(), invoice_id.clone(), amount_refunded),
    );
}

/// Publish settlement_paid event per investor.
pub fn settlement_paid(
    env: &Env,
    investor: &Address,
    invoice_id: &soroban_sdk::BytesN<32>,
    payout_amount: i128,
    yield_earned: i128,
) {
    env.events().publish(
        (Symbol::new(env, "settlement_paid"),),
        (
            investor.clone(),
            invoice_id.clone(),
            payout_amount,
            yield_earned,
        ),
    );
}

pub fn settlement_proposed(env: &Env, invoice_id: &BytesN<32>, proposer: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "settlement_proposed"),),
        (invoice_id, proposer, amount),
    );
}

pub fn settlement_approved(env: &Env, invoice_id: &BytesN<32>, approver: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "settlement_approved"),),
        (invoice_id, approver, amount),
    );
}

/// Publish grace_period_updated event when the admin changes the grace window.
pub fn grace_period_updated(env: &Env, old_seconds: u64, new_seconds: u64) {
    env.events().publish(
        (Symbol::new(env, "grace_period_updated"),),
        (old_seconds, new_seconds),
    );
}

/// Publish `GracePeriodExpired` when `refund_escrow` succeeds specifically
/// because the grace window (not just the bare due date) has lapsed.
pub fn grace_period_expired(env: &Env, inv_id: Symbol, due_dt: u64, grace_period_seconds: u64) {
    env.events().publish(
        (Symbol::new(env, "GracePeriodExpired"),),
        (inv_id, due_dt, grace_period_seconds),
    );
}

/// Publish category_fee_updated event when the admin sets a category's fee rate.
pub fn category_fee_updated(env: &Env, category: crate::types::InvoiceCategory, fee_bps: u32) {
    env.events().publish(
        (Symbol::new(env, "category_fee_updated"),),
        (category as u32, fee_bps),
    );
}

/// Publish `DisputeRaised` when `raise_dispute` transitions an escrow to `Disputed`.
pub fn dispute_raised(env: &Env, inv_id: Symbol, raiser: &Address, raised_at: u64) {
    env.events().publish(
        (Symbol::new(env, "DisputeRaised"),),
        (inv_id, raiser.clone(), raised_at),
    );
}

/// Publish `DisputeResolved` when `resolve_dispute` settles or refunds a disputed escrow.
/// `timed_out` is true when resolution happened via the timeout fallback rather
/// than an explicit `favour` decision.
pub fn dispute_resolved(env: &Env, inv_id: Symbol, favour: Symbol, timed_out: bool) {
    env.events().publish(
        (Symbol::new(env, "DisputeResolved"),),
        (inv_id, favour, timed_out),
    );
}

/// Publish `installment_schedule_set` when a seller configures (or replaces)
/// the installment repayment milestone schedule for an invoice.
pub fn installment_schedule_set(
    env: &Env,
    inv_id: Symbol,
    milestone_count: u32,
    final_due_ts: u64,
    total_amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "installment_schedule_set"),),
        (inv_id, milestone_count, final_due_ts, total_amount),
    );
}

/// Publish `installment_settled` when cumulative repayments reach a
/// milestone's cumulative target (or the escrow fully settles).
pub fn installment_settled(
    env: &Env,
    inv_id: Symbol,
    index: u32,
    cumulative_amount: i128,
    paid_amt: i128,
) {
    env.events().publish(
        (Symbol::new(env, "installment_settled"),),
        (inv_id, index, cumulative_amount, paid_amt),
    );
}

/// Publish the protocol fee collected from a repayment and its treasury.
pub fn fee_collected(env: &Env, inv_id: Symbol, fee_amount: i128, treasury: &Address) {
    env.events().publish(
        (Symbol::new(env, "fee_collected"),),
        (inv_id, fee_amount, treasury.clone()),
    );
}
/// Publish invoice cancellation with the authorized admin as the actor.
pub fn invoice_cancelled(env: &Env, invoice_id: &BytesN<32>, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "invoice_cancelled"), invoice_id.clone()),
        admin.clone(),
    );
}

/// Publish a change to the maximum unique investors allowed per invoice.
pub fn max_investors_updated(env: &Env, count: u32) {
    env.events()
        .publish((Symbol::new(env, "max_investors_updated"),), count);
}

// ── Issue #443: Timelock controller events ────────────────────────────────────

/// Emitted when an admin proposes a timelocked parameter change.
pub fn param_change_proposed(env: &Env, proposal: &crate::types::PendingParamChange) {
    env.events().publish(
        (Symbol::new(env, "param_change_proposed"),),
        (
            proposal.param as u32,
            proposal.new_value,
            proposal.proposed_at,
            proposal.timelock_secs,
        ),
    );
}

/// Emitted when a timelocked parameter change is executed after the delay.
pub fn param_change_executed(env: &Env, proposal: &crate::types::PendingParamChange) {
    env.events().publish(
        (Symbol::new(env, "param_change_executed"),),
        (proposal.param as u32, proposal.new_value),
    );
}

// ── Multi-currency investment events ──────────────────────────────────────────

/// Publish a multi-currency investment with its normalised base amount.
pub fn currency_invested(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    currency: &Symbol,
    amount: i128,
    base_amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "currency_invested"), invoice_id.clone()),
        (investor.clone(), currency.clone(), amount, base_amount),
    );
}
