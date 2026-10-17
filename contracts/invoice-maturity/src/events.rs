#![allow(deprecated)]
//! Event definitions for the invoice maturity contract.

use soroban_sdk::{Env, Symbol};

/// Emitted when the admin configures an invoice's maturity timestamp.
pub fn maturity_set(env: &Env, invoice_id: Symbol, maturity_timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "maturity_set"),),
        (invoice_id, maturity_timestamp),
    );
}

/// Emitted when `trigger_maturity` runs on a mature invoice, carrying the
/// invoice id and its maturity timestamp.
pub fn maturity_reached(env: &Env, invoice_id: Symbol, maturity_timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "maturity_reached"),),
        (invoice_id, maturity_timestamp),
    );
}

/// Emitted alongside `MaturityReached` to mark the settlement flow start.
pub fn settlement_triggered(env: &Env, invoice_id: Symbol, triggered_at: u64) {
    env.events().publish(
        (Symbol::new(env, "settlement_triggered"),),
        (invoice_id, triggered_at),
    );
}
