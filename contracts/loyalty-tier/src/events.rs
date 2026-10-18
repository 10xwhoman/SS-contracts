use crate::types::Tier;
use soroban_sdk::{Address, Env, Symbol};

pub fn loyalty_tier_upgraded(
    env: &Env,
    wallet: &Address,
    old_tier: Tier,
    new_tier: Tier,
    cumulative_volume: i128,
) {
    let topics = (Symbol::new(env, "LoyaltyTierUpgraded"), wallet.clone());
    env.events().publish(
        topics,
        (old_tier as u32, new_tier as u32, cumulative_volume),
    );
}

pub fn loyalty_thresholds_updated(env: &Env, silver: i128, gold: i128, platinum: i128) {
    let topics = (Symbol::new(env, "LoyaltyThresholdsUpdated"),);
    env.events().publish(topics, (silver, gold, platinum));
}
