#![allow(deprecated)]
//! Event definitions for the waitlist contract.

use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a wallet joins the waitlist.
pub fn waitlist_joined(env: &Env, wallet: &Address, position: u32) {
    env.events()
        .publish((Symbol::new(env, "waitlist_joined"),), (wallet, position));
}

/// Emitted when a wallet is approved and promoted to the KYC whitelist.
pub fn waitlist_approved(env: &Env, wallet: &Address, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "waitlist_approved"),),
        (wallet, timestamp),
    );
}
