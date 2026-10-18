#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, Env};
pub use types::{LoyaltyInfo, LoyaltyThresholds, StorageKey, Tier};

#[contract]
pub struct LoyaltyTier;

fn default_thresholds() -> LoyaltyThresholds {
    LoyaltyThresholds {
        silver: 10_000,
        gold: 100_000,
        platinum: 500_000,
    }
}

fn valid_thresholds(t: &LoyaltyThresholds) -> bool {
    t.silver > 0 && t.silver < t.gold && t.gold < t.platinum
}

fn resolve_tier(volume: i128, t: &LoyaltyThresholds) -> Tier {
    if volume >= t.platinum {
        Tier::Platinum
    } else if volume >= t.gold {
        Tier::Gold
    } else if volume >= t.silver {
        Tier::Silver
    } else {
        Tier::Bronze
    }
}

fn next_threshold(tier: Tier, t: &LoyaltyThresholds) -> Option<i128> {
    match tier {
        Tier::Bronze => Some(t.silver),
        Tier::Silver => Some(t.gold),
        Tier::Gold => Some(t.platinum),
        Tier::Platinum => None,
    }
}

fn load_thresholds(env: &Env) -> Result<LoyaltyThresholds, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Thresholds)
        .ok_or(Error::NotInitialized)
}

fn load_volume(env: &Env, wallet: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Volume(wallet.clone()))
        .unwrap_or(0)
}

#[contractimpl]
impl LoyaltyTier {
    pub fn initialize(env: Env, admin: Address, escrow: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Escrow, &escrow);
        env.storage()
            .instance()
            .set(&StorageKey::Thresholds, &default_thresholds());
        Ok(())
    }

    /// Called by the authorised escrow contract on each investment.
    pub fn update_loyalty(
        env: Env,
        wallet: Address,
        investment_amount: i128,
    ) -> Result<Tier, Error> {
        let escrow: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Escrow)
            .ok_or(Error::NotInitialized)?;
        escrow.require_auth();

        if investment_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let thresholds = load_thresholds(&env)?;
        let old_volume = load_volume(&env, &wallet);
        let new_volume = old_volume
            .checked_add(investment_amount)
            .ok_or(Error::Overflow)?;
        env.storage()
            .persistent()
            .set(&StorageKey::Volume(wallet.clone()), &new_volume);

        let old_tier = resolve_tier(old_volume, &thresholds);
        let new_tier = resolve_tier(new_volume, &thresholds);
        if new_tier > old_tier {
            events::loyalty_tier_upgraded(&env, &wallet, old_tier, new_tier, new_volume);
        }

        Ok(new_tier)
    }

    pub fn set_loyalty_thresholds(env: Env, thresholds: LoyaltyThresholds) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if !valid_thresholds(&thresholds) {
            return Err(Error::InvalidThresholds);
        }

        env.storage()
            .instance()
            .set(&StorageKey::Thresholds, &thresholds);
        events::loyalty_thresholds_updated(
            &env,
            thresholds.silver,
            thresholds.gold,
            thresholds.platinum,
        );
        Ok(())
    }

    pub fn get_loyalty_thresholds(env: Env) -> Result<LoyaltyThresholds, Error> {
        load_thresholds(&env)
    }

    pub fn get_loyalty_tier(env: Env, wallet: Address) -> Result<LoyaltyInfo, Error> {
        let thresholds = load_thresholds(&env)?;
        let cumulative_volume = load_volume(&env, &wallet);
        let tier = resolve_tier(cumulative_volume, &thresholds);
        Ok(LoyaltyInfo {
            tier,
            cumulative_volume,
            next_threshold: next_threshold(tier, &thresholds),
        })
    }
}
