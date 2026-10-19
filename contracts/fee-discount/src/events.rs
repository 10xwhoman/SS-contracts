use crate::types::LoyaltyTier;
use soroban_sdk::{Address, Env, Symbol};

/// Emitted each time a discounted fee is applied to invest() or buy().
pub fn fee_discount_applied(
    env: &Env,
    wallet: &Address,
    tier: LoyaltyTier,
    original_fee: i128,
    discount_amount: i128,
    final_fee: i128,
) {
    let topics = (Symbol::new(env, "FeeDiscountApplied"), wallet.clone());
    env.events()
        .publish(topics, (tier as u32, original_fee, discount_amount, final_fee));
}
