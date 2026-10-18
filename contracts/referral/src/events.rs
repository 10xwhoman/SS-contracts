#![allow(deprecated)]
//! Event definitions for the referral contract.

use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a referral fee is accrued to a referrer.
/// Carries the investment ID, referee, referrer, and accrued amount.
pub fn referral_fee_paid(
    env: &Env,
    invoice_id: soroban_sdk::Symbol,
    referee: &Address,
    referrer: &Address,
    amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "referral_fee_paid"),),
        (invoice_id, referee, referrer, amount),
    );
}

/// Emitted when a referral relationship is registered.
pub fn referral_registered(env: &Env, referee: &Address, referrer: &Address) {
    env.events().publish(
        (Symbol::new(env, "referral_registered"),),
        (referee, referrer),
    );
}

/// Emitted when a referrer claims their accumulated rewards.
pub fn rewards_claimed(env: &Env, referrer: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "rewards_claimed"),), (referrer, amount));
}

/// Emitted when the referral fee rate is updated.
pub fn fee_rate_updated(env: &Env, old_bps: u32, new_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "fee_rate_updated"),), (old_bps, new_bps));
}
