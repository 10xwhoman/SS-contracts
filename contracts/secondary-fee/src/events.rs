#![allow(deprecated)]
//! Events emitted by the secondary market fee contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::types::{FeeReceipt, Sector};

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

/// Emitted when the invoice registry used to resolve sectors changes.
pub fn invoice_registry_updated(env: &Env, old_registry: Option<Address>, new_registry: &Address) {
    let topics = (Symbol::new(env, "invoice_registry_updated"),);
    env.events()
        .publish(topics, (old_registry, new_registry.clone()));
}

/// Emitted when the per-fraction listing fee for a sector changes.
pub fn listing_fee_updated(env: &Env, sector: Sector, old_fee: i128, new_fee: i128) {
    let topics = (Symbol::new(env, "listing_fee_updated"), sector as u32);
    env.events().publish(topics, (old_fee, new_fee));
}

/// Emitted whenever a listing fee is collected from a seller.
/// Topics: (ListingFeeCollected, invoice_id, seller);
/// Data: (token, amount, fee_amount, timestamp).
pub fn listing_fee_collected(
    env: &Env,
    invoice_id: &BytesN<32>,
    seller: &Address,
    token: &Address,
    amount: i128,
    fee_amount: i128,
) {
    let topics = (
        Symbol::new(env, "ListingFeeCollected"),
        invoice_id.clone(),
        seller.clone(),
    );
    env.events().publish(
        topics,
        (token.clone(), amount, fee_amount, env.ledger().timestamp()),
    );
}
