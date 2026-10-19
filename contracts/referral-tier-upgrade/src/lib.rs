//! Referral tier upgrade contract for StellarSettle (issue #620).
//!
//! Upgrades referrers to higher reward tiers as their cumulative referred
//! investment volume grows, unlocking higher commission rates for top referrers.
//!
//! Tier schedule (cumulative referred volume):
//!   Standard  –    0 – 49 999 → 20% fee share
//!   Plus      – 50 000 – 499 999 → 25% fee share
//!   Elite     – 500 000+          → 30% fee share

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{commission_bps, Config, ReferralInfo, ReferralTier, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, Env};

use types::{next_threshold, tier_from_volume};

fn load_config(env: &Env) -> Result<Config, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .ok_or(Error::NotInitialized)
}

fn load_volume(env: &Env, wallet: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Volume(wallet.clone()))
        .unwrap_or(0)
}

fn load_tier(env: &Env, wallet: &Address) -> ReferralTier {
    env.storage()
        .persistent()
        .get(&StorageKey::Tier(wallet.clone()))
        .unwrap_or(ReferralTier::Standard)
}

#[contract]
pub struct ReferralTierUpgrade;

#[contractimpl]
impl ReferralTierUpgrade {
    /// Initialise the contract.
    ///
    /// * `admin`            – can update configuration.
    /// * `referral_contract`– the only caller allowed to call
    ///                        `on_referral_reward_paid`.
    pub fn initialize(
        env: Env,
        admin: Address,
        referral_contract: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(
            &StorageKey::Config,
            &Config {
                admin,
                referral_contract,
            },
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Hook called by the referral contract
    // -----------------------------------------------------------------------

    /// Called by the bound referral contract whenever a `ReferralRewardPaid`
    /// event fires, adding `referred_volume` to the referrer's cumulative
    /// total and upgrading their tier if they cross a boundary.
    pub fn on_referral_reward_paid(
        env: Env,
        referrer: Address,
        referred_volume: i128,
    ) -> Result<ReferralTier, Error> {
        let cfg = load_config(&env)?;
        cfg.referral_contract.require_auth();

        if referred_volume <= 0 {
            return Err(Error::InvalidAmount);
        }

        let old_volume = load_volume(&env, &referrer);
        let new_volume = old_volume
            .checked_add(referred_volume)
            .ok_or(Error::Overflow)?;

        env.storage()
            .persistent()
            .set(&StorageKey::Volume(referrer.clone()), &new_volume);

        let old_tier = load_tier(&env, &referrer);
        let new_tier = tier_from_volume(new_volume);

        env.storage()
            .persistent()
            .set(&StorageKey::Tier(referrer.clone()), &new_tier);

        if new_tier > old_tier {
            events::referral_tier_upgraded(&env, &referrer, old_tier, new_tier, new_volume);
        }

        Ok(new_tier)
    }

    // -----------------------------------------------------------------------
    // Explicit update entry point
    // -----------------------------------------------------------------------

    /// Recomputes and persists the tier for `wallet` based on their current
    /// cumulative volume. Can be called by anyone (no auth required) since
    /// it only reads existing state.
    pub fn update_referral_tier(env: Env, wallet: Address) -> Result<ReferralTier, Error> {
        load_config(&env)?;
        let volume = load_volume(&env, &wallet);
        let old_tier = load_tier(&env, &wallet);
        let new_tier = tier_from_volume(volume);

        env.storage()
            .persistent()
            .set(&StorageKey::Tier(wallet.clone()), &new_tier);

        if new_tier > old_tier {
            events::referral_tier_upgraded(&env, &wallet, old_tier, new_tier, volume);
        }

        Ok(new_tier)
    }

    // -----------------------------------------------------------------------
    // Read helpers
    // -----------------------------------------------------------------------

    /// Returns the current tier, cumulative referred volume, and next tier
    /// threshold for `wallet`.
    pub fn get_referral_tier(env: Env, wallet: Address) -> Result<ReferralInfo, Error> {
        load_config(&env)?;
        let volume = load_volume(&env, &wallet);
        let tier = tier_from_volume(volume);
        Ok(ReferralInfo {
            tier,
            cumulative_volume: volume,
            next_tier_threshold: next_threshold(tier),
        })
    }

    /// Returns the commission rate (in bps) for `wallet`'s current tier.
    pub fn get_commission_bps(env: Env, wallet: Address) -> Result<i128, Error> {
        load_config(&env)?;
        let volume = load_volume(&env, &wallet);
        let tier = tier_from_volume(volume);
        Ok(commission_bps(tier))
    }
}
