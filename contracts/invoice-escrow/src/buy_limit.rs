//! Per-wallet buy limits on secondary-market purchases of a funding invoice (#547).
//!
//! A wallet may acquire at most `max_amount` of an invoice's position within a
//! rolling `window_seconds` window. The window starts at the wallet's first
//! purchase and resets automatically once it has elapsed.

use soroban_sdk::{contracttype, Address, BytesN, Env};

use crate::errors::Error;
use crate::storage::bump_persistent;
use crate::types::StorageKey;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuyLimit {
    pub max_amount: i128,
    pub window_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuyWindow {
    pub window_start: u64,
    pub bought: i128,
}

pub fn set_limit(env: &Env, invoice_id: &BytesN<32>, limit: &BuyLimit) {
    let key = StorageKey::BuyLimit(invoice_id.clone());
    env.storage().persistent().set(&key, limit);
    bump_persistent(env, &key);
}

pub fn get_limit(env: &Env, invoice_id: &BytesN<32>) -> Option<BuyLimit> {
    env.storage()
        .persistent()
        .get(&StorageKey::BuyLimit(invoice_id.clone()))
}

/// The wallet's window as of now, or a fresh one if none exists or it expired.
fn current_window(env: &Env, invoice_id: &BytesN<32>, wallet: &Address, limit: &BuyLimit) -> BuyWindow {
    let now = env.ledger().timestamp();
    let key = StorageKey::BuyWindow(invoice_id.clone(), wallet.clone());
    match env.storage().persistent().get::<_, BuyWindow>(&key) {
        Some(window) if now < window.window_start.saturating_add(limit.window_seconds) => window,
        _ => BuyWindow { window_start: now, bought: 0 },
    }
}

/// Amount the wallet may still buy in its current window; `i128::MAX` if no limit is set.
pub fn remaining(env: &Env, invoice_id: &BytesN<32>, wallet: &Address) -> i128 {
    match get_limit(env, invoice_id) {
        Some(limit) => {
            let window = current_window(env, invoice_id, wallet, &limit);
            (limit.max_amount - window.bought).max(0)
        }
        None => i128::MAX,
    }
}

/// Count `amount` against the wallet's window, failing with `BuyLimitExceeded`
/// if it would push cumulative purchases past the limit.
pub fn record_purchase(env: &Env, invoice_id: &BytesN<32>, wallet: &Address, amount: i128) -> Result<(), Error> {
    let Some(limit) = get_limit(env, invoice_id) else {
        return Ok(());
    };
    let mut window = current_window(env, invoice_id, wallet, &limit);
    let bought = window.bought.checked_add(amount).ok_or(Error::Overflow)?;
    if bought > limit.max_amount {
        return Err(Error::BuyLimitExceeded);
    }
    window.bought = bought;
    let key = StorageKey::BuyWindow(invoice_id.clone(), wallet.clone());
    env.storage().persistent().set(&key, &window);
    bump_persistent(env, &key);
    Ok(())
}
