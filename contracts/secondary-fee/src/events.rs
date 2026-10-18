#![allow(deprecated)]
//! Events emitted by the secondary market fee contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::FeeReceipt;

/// Emitted when the contract is initialized.
pub fn initialized(env: &Env, admin: &Address, treasury: &Address, fee_bps: u32) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events()
        .publish(topics, (admin.clone(), treasury.clone(), fee_bps));
}

/// Emitted when the platform fee rate changes.
pub fn fee_bps_updated(env: &Env, old_bps: u32, new_bps: u32) {
    let topics = (Symbol::new(env, "fee_bps_updated"),);
    env.events().publish(topics, (old_bps, new_bps));
}

/// Emitted when the treasury address changes.
pub fn treasury_updated(env: &Env, old_treasury: Option<Address>, new_treasury: &Address) {
    let topics = (Symbol::new(env, "treasury_updated"),);
    env.events()
        .publish(topics, (old_treasury, new_treasury.clone()));
}

/// Emitted when the authorized secondary market contract changes.
pub fn secondary_market_updated(env: &Env, old_market: Option<Address>, new_market: &Address) {
    let topics = (Symbol::new(env, "secondary_market_updated"),);
    env.events()
        .publish(topics, (old_market, new_market.clone()));
}

/// Emitted for every secondary market sale the fee is collected from.
/// Topics: (SecondaryFeeCollected, listing_id, seller);
/// Data: (token, sale_amount, fee_amount, net_amount, timestamp).
pub fn secondary_fee_collected(
    env: &Env,
    listing_id: &Symbol,
    seller: &Address,
    token: &Address,
    receipt: &FeeReceipt,
) {
    let topics = (
        Symbol::new(env, "SecondaryFeeCollected"),
        listing_id.clone(),
        seller.clone(),
    );
    env.events().publish(
        topics,
        (
            token.clone(),
            receipt.sale_amount,
            receipt.fee_amount,
            receipt.net_amount,
            env.ledger().timestamp(),
        ),
    );
}
