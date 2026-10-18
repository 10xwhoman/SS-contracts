#![allow(deprecated)]
//! Event definitions for the fee configuration contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::FeeType;

/// Emit a `FeeUpdated` event whenever a fee rate changes.
///
/// Topics: (contract, event_name, fee_type)
/// Data:   (old_bps, new_bps, admin, timestamp)
pub fn fee_updated(
    env: &Env,
    fee_type: FeeType,
    old_bps: u32,
    new_bps: u32,
    admin: &Address,
    timestamp: u64,
) {
    env.events().publish(
        (
            Symbol::new(env, "FeeUpdated"),
            fee_type as u32,
        ),
        (old_bps, new_bps, admin.clone(), timestamp),
    );
}

/// Emit a `BoundsUpdated` event when admin changes the fee bounds.
pub fn bounds_updated(env: &Env, old_min: u32, old_max: u32, new_min: u32, new_max: u32) {
    env.events().publish(
        (Symbol::new(env, "BoundsUpdated"),),
        (old_min, old_max, new_min, new_max),
    );
}
