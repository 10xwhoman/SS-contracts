use crate::types::ReferralTier;
use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a wallet crosses a referral tier boundary.
pub fn referral_tier_upgraded(
    env: &Env,
    wallet: &Address,
    old_tier: ReferralTier,
    new_tier: ReferralTier,
    cumulative_volume: i128,
) {
    let topics = (Symbol::new(env, "ReferralTierUpgraded"), wallet.clone());
    env.events()
        .publish(topics, (old_tier as u32, new_tier as u32, cumulative_volume));
}
