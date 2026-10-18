#![allow(deprecated)]
//! Events emitted by the portfolio snapshot contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::TokenEntry;

/// Emitted when the contract is initialized.
pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted for every recorded portfolio snapshot.
/// Topics: (PortfolioSnapshotTaken, wallet); Data: (snapshot_id, timestamp).
pub fn snapshot_taken(env: &Env, wallet: &Address, snapshot_id: u64, timestamp: u64) {
    let topics = (Symbol::new(env, "PortfolioSnapshotTaken"), wallet.clone());
    env.events().publish(topics, (snapshot_id, timestamp));
}

/// Emitted when an invoice-token contract is registered or replaced.
pub fn token_registered(env: &Env, entry: &TokenEntry) {
    let topics = (Symbol::new(env, "token_registered"),);
    env.events()
        .publish(topics, (entry.invoice_id.clone(), entry.token.clone()));
}

/// Emitted when an invoice-token contract is removed from the registry.
pub fn token_removed(env: &Env, invoice_id: &Symbol, token: &Address) {
    let topics = (Symbol::new(env, "token_removed"),);
    env.events()
        .publish(topics, (invoice_id.clone(), token.clone()));
}

/// Emitted when expired snapshots are pruned for a wallet.
pub fn snapshots_pruned(env: &Env, wallet: &Address, removed: u32) {
    let topics = (Symbol::new(env, "snapshots_pruned"), wallet.clone());
    env.events().publish(topics, removed);
}
