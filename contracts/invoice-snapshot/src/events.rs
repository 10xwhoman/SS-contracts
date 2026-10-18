#![allow(deprecated)]
//! Event definitions for the investor position snapshot contract.

use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a snapshot is recorded, carrying the invoice id and the block
/// height (ledger sequence) it was taken at.
pub fn snapshot_taken(env: &Env, invoice_id: Symbol, snapshot_id: u32, ledger_sequence: u32) {
    env.events().publish(
        (Symbol::new(env, "snapshot_taken"),),
        (invoice_id, snapshot_id, ledger_sequence),
    );
}

/// Emitted when snapshots past the 90-day retention window are pruned.
pub fn snapshots_pruned(env: &Env, invoice_id: Symbol, pruned_count: u32, remaining: u32) {
    env.events().publish(
        (Symbol::new(env, "snapshots_pruned"),),
        (invoice_id, pruned_count, remaining),
    );
}

/// Emitted when a pro-rata distribution is routed to a snapshot's holders.
pub fn snapshot_distributed(
    env: &Env,
    invoice_id: Symbol,
    snapshot_id: u32,
    token: Address,
    total_amount: i128,
    distributed: i128,
) {
    env.events().publish(
        (Symbol::new(env, "snapshot_distributed"),),
        (invoice_id, snapshot_id, token, total_amount, distributed),
    );
}
