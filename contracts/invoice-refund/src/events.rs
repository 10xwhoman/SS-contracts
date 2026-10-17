#![allow(deprecated)]
//! Event definitions for the invoice refund contract.
//!
//! Events follow the repository convention: a single event-name topic plus a
//! data payload carrying the invoice id and all amounts.

use soroban_sdk::{Address, Env, Symbol};

/// Emitted when the admin registers an invoice with a funding threshold and expiry.
pub fn invoice_registered(
    env: &Env,
    invoice_id: Symbol,
    funding_threshold: i128,
    expiry_timestamp: u64,
) {
    env.events().publish(
        (Symbol::new(env, "invoice_registered"),),
        (invoice_id, funding_threshold, expiry_timestamp),
    );
}

/// Emitted when an investor locks USDC into an invoice.
pub fn investment_recorded(
    env: &Env,
    invoice_id: Symbol,
    investor: &Address,
    amount: i128,
    total_locked: i128,
) {
    env.events().publish(
        (Symbol::new(env, "investment_recorded"),),
        (invoice_id, investor.clone(), amount, total_locked),
    );
}

/// Emitted once per investor when their pro-rata locked amount is returned.
pub fn refund_processed(env: &Env, invoice_id: Symbol, investor: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "refund_processed"),),
        (invoice_id, investor.clone(), amount),
    );
}

/// Emitted after every investor position has been refunded.
pub fn invoice_refund_complete(
    env: &Env,
    invoice_id: Symbol,
    total_refunded: i128,
    investor_count: u32,
) {
    env.events().publish(
        (Symbol::new(env, "invoice_refund_complete"),),
        (invoice_id, total_refunded, investor_count),
    );
}
