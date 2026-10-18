#![allow(deprecated)]

use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::FeeTier;

pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Issue #122: Fee recipient updated event
pub fn fee_recipient_updated(env: &Env, old_recipient: Option<Address>, new_recipient: &Address) {
    let topics = (Symbol::new(env, "fee_recipient_updated"),);
    env.events()
        .publish(topics, (old_recipient, new_recipient.clone()));
}

/// Issue #121: Escrow contract binding updated event.
pub fn escrow_contract_updated(env: &Env, old_escrow: Option<Address>, new_escrow: &Address) {
    let topics = (Symbol::new(env, "escrow_contract_updated"),);
    env.events()
        .publish(topics, (old_escrow, new_escrow.clone()));
}

/// Issue #123: Enhanced structured payment distribution audit event.
/// Emits comprehensive distribution details for compliance and audit trails.
///
/// Event structure:
/// - Topics: (PaymentDistributed, escrow_address, invoice_id)
/// - Data: (
///   recipients: Vec<Address>,  // [seller, funder, fee_recipient]
///   amounts: Vec<i128>,        // [seller_amount, investor_amount, platform_fee, total_paid]
///   escrow_status: u32,        // Current escrow status
///   timestamp: u64             // Ledger timestamp
///   )
pub fn payment_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    escrow_status: u32,
) {
    let topics = (
        Symbol::new(env, "PaymentDistributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    // Issue #123: Add escrow_status and timestamp for comprehensive audit trail
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            recipients.clone(),
            amounts.clone(),
            escrow_status,
            timestamp,
        ),
    );
}

pub fn refund_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
) {
    let topics = (
        Symbol::new(env, "refund_distributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (recipients.clone(), amounts.clone()));
}

#[allow(dead_code)]
pub fn platform_fee_updated(env: &Env, admin: &Address, tiers: &Vec<FeeTier>) {
    let topics = (Symbol::new(env, "platform_fee_updated"),);
    env.events().publish(topics, (admin.clone(), tiers.clone()));
}

pub fn investor_bonus_rate_updated(env: &Env, admin: &Address, bonus_bps: u32) {
    let topics = (Symbol::new(env, "investor_bonus_rate_updated"),);
    env.events().publish(topics, (admin.clone(), bonus_bps));
}

/// Issue #130: Referral fee cut payout event.
/// Topics: (referral_paid, token); Data: (referral_recipient, amount).
pub fn referral_paid(env: &Env, token: &Address, referral: &Address, amount: i128) {
    let topics = (Symbol::new(env, "referral_paid"), token.clone());
    env.events().publish(topics, (referral.clone(), amount));
}

/// Issue #126: Per-asset multi-currency distribution audit event.
/// Topics: (AssetDistributed, token); Data: (recipients, amounts, total).
pub fn asset_distributed(
    env: &Env,
    token: &Address,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    total: i128,
) {
    let topics = (Symbol::new(env, "AssetDistributed"), token.clone());
    env.events()
        .publish(topics, (recipients.clone(), amounts.clone(), total));
}

/// Issue #125: Emergency withdrawal audit event.
/// Topics: (EmergencyWithdrawal, token); Data: (admin, to, amount).
pub fn emergency_withdrawal(
    env: &Env,
    admin: &Address,
    token: &Address,
    to: &Address,
    amount: i128,
) {
    let topics = (Symbol::new(env, "EmergencyWithdrawal"), token.clone());
    env.events()
        .publish(topics, (admin.clone(), to.clone(), amount));
}

/// Issue #182: Role grant/revoke event.
#[allow(dead_code)]
pub fn role_grant_updated(env: &Env, role: &Symbol, account: &Address, granted: bool) {
    let topics = (
        Symbol::new(env, "role_grant_updated"),
        role.clone(),
        account.clone(),
    );
    env.events().publish(topics, granted);
}

/// Issue #119: Dust amount swept event.
/// Topics: (DustSwept, token); Data: (admin, to, amount).
pub fn dust_swept(env: &Env, admin: &Address, token: &Address, to: &Address, amount: i128) {
    let topics = (Symbol::new(env, "DustSwept"), token.clone());
    env.events()
        .publish(topics, (admin.clone(), to.clone(), amount));
}

/// Issue #482: excess funds returned to the escrow that deposited them.
pub fn excess_refunded(
    env: &Env,
    escrow: &Address,
    token: &Address,
    invoice_id: &Symbol,
    amount: i128,
) {
    let topics = (
        Symbol::new(env, "ExcessRefunded"),
        token.clone(),
        invoice_id.clone(),
    );
    env.events().publish(topics, (escrow.clone(), amount));
}
