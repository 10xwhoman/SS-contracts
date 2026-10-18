#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, Env};
pub use types::{InvestorMetrics, InvestorScore, StorageKey, Tier, TierThresholds};

#[contract]
pub struct InvestorScoring;

fn default_thresholds() -> TierThresholds {
    TierThresholds {
        silver: 300,
        gold: 600,
        platinum: 850,
    }
}

fn compute_score_from_metrics(metrics: &InvestorMetrics) -> u32 {
    let base_score: i32 = 100;

    // Investment count: up to 300 points (15 pts per investment)
    let inv_points: i32 = ((metrics.investment_count as i32) * 15).min(300);

    // Total volume: up to 350 points (scaled by stroops)
    let vol_scaled: i32 = if metrics.total_volume > 0 {
        ((metrics.total_volume / 10_000_000) as i32 * 5).min(350)
    } else {
        0
    };

    // Default penalty: 50 points per default
    let default_penalty: i32 = (metrics.default_exposure as i32) * 50;

    // Early exit penalty: 20 points per early exit
    let exit_penalty: i32 = (metrics.early_exit_count as i32) * 20;

    let raw_score = base_score + inv_points + vol_scaled - default_penalty - exit_penalty;

    if raw_score <= 0 {
        0
    } else if raw_score >= 1000 {
        1000
    } else {
        raw_score as u32
    }
}

fn resolve_tier(score: u32, thresholds: &TierThresholds) -> Tier {
    if score >= thresholds.platinum {
        Tier::Platinum
    } else if score >= thresholds.gold {
        Tier::Gold
    } else if score >= thresholds.silver {
        Tier::Silver
    } else {
        Tier::Bronze
    }
}

#[contractimpl]
impl InvestorScoring {
    pub fn initialize(
        env: Env,
        admin: Address,
        thresholds: Option<TierThresholds>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        let thresh = thresholds.unwrap_or_else(default_thresholds);
        if thresh.silver >= thresh.gold || thresh.gold >= thresh.platinum {
            return Err(Error::InvalidThresholds);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::TierThresholds, &thresh);

        Ok(())
    }

    pub fn set_authorized_contract(
        env: Env,
        caller: Address,
        authorized: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&StorageKey::Authorized(caller.clone()), &authorized);
        events::authorized_caller_updated(&env, &caller, authorized);

        Ok(())
    }

    pub fn is_authorized(env: Env, caller: Address) -> bool {
        let admin: Option<Address> = env.storage().instance().get(&StorageKey::Admin);
        if let Some(adm) = admin {
            if adm == caller {
                return true;
            }
        }
        env.storage()
            .instance()
            .get::<_, bool>(&StorageKey::Authorized(caller))
            .unwrap_or(false)
    }

    pub fn set_tier_thresholds(env: Env, thresholds: TierThresholds) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if thresholds.silver >= thresholds.gold || thresholds.gold >= thresholds.platinum {
            return Err(Error::InvalidThresholds);
        }

        env.storage()
            .instance()
            .set(&StorageKey::TierThresholds, &thresholds);
        events::tier_thresholds_updated(
            &env,
            thresholds.silver,
            thresholds.gold,
            thresholds.platinum,
        );

        Ok(())
    }

    pub fn get_tier_thresholds(env: Env) -> Result<TierThresholds, Error> {
        env.storage()
            .instance()
            .get(&StorageKey::TierThresholds)
            .ok_or(Error::NotInitialized)
    }

    pub fn record_activity(
        env: Env,
        caller: Address,
        wallet: Address,
        investment_count_delta: u32,
        default_exposure_delta: u32,
        early_exit_delta: u32,
        volume_delta: i128,
    ) -> Result<u32, Error> {
        if !Self::is_authorized(env.clone(), caller.clone()) {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        let mut metrics = env
            .storage()
            .persistent()
            .get(&StorageKey::Metrics(wallet.clone()))
            .unwrap_or(InvestorMetrics {
                investment_count: 0,
                default_exposure: 0,
                early_exit_count: 0,
                total_volume: 0,
            });

        metrics.investment_count = metrics
            .investment_count
            .saturating_add(investment_count_delta);
        metrics.default_exposure = metrics
            .default_exposure
            .saturating_add(default_exposure_delta);
        metrics.early_exit_count = metrics
            .early_exit_count
            .saturating_add(early_exit_delta);
        metrics.total_volume = metrics.total_volume.saturating_add(volume_delta);

        env.storage()
            .persistent()
            .set(&StorageKey::Metrics(wallet.clone()), &metrics);

        Self::update_score_internal(&env, &wallet, &metrics)
    }

    pub fn set_metrics(
        env: Env,
        caller: Address,
        wallet: Address,
        metrics: InvestorMetrics,
    ) -> Result<u32, Error> {
        if !Self::is_authorized(env.clone(), caller.clone()) {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        env.storage()
            .persistent()
            .set(&StorageKey::Metrics(wallet.clone()), &metrics);

        Self::update_score_internal(&env, &wallet, &metrics)
    }

    pub fn update_score(env: Env, wallet: Address) -> Result<u32, Error> {
        let metrics = env
            .storage()
            .persistent()
            .get(&StorageKey::Metrics(wallet.clone()))
            .unwrap_or(InvestorMetrics {
                investment_count: 0,
                default_exposure: 0,
                early_exit_count: 0,
                total_volume: 0,
            });

        Self::update_score_internal(&env, &wallet, &metrics)
    }

    fn update_score_internal(
        env: &Env,
        wallet: &Address,
        metrics: &InvestorMetrics,
    ) -> Result<u32, Error> {
        let thresholds: TierThresholds = env
            .storage()
            .instance()
            .get(&StorageKey::TierThresholds)
            .ok_or(Error::NotInitialized)?;

        let old_score = env
            .storage()
            .persistent()
            .get::<_, u32>(&StorageKey::Score(wallet.clone()))
            .unwrap_or(100);

        let new_score = compute_score_from_metrics(metrics);
        let tier = resolve_tier(new_score, &thresholds);

        env.storage()
            .persistent()
            .set(&StorageKey::Score(wallet.clone()), &new_score);

        events::score_updated(env, wallet, old_score, new_score, tier);

        Ok(new_score)
    }

    pub fn get_score(env: Env, wallet: Address) -> Result<InvestorScore, Error> {
        let thresholds: TierThresholds = env
            .storage()
            .instance()
            .get(&StorageKey::TierThresholds)
            .ok_or(Error::NotInitialized)?;

        let metrics = env
            .storage()
            .persistent()
            .get(&StorageKey::Metrics(wallet.clone()))
            .unwrap_or(InvestorMetrics {
                investment_count: 0,
                default_exposure: 0,
                early_exit_count: 0,
                total_volume: 0,
            });

        let score = env
            .storage()
            .persistent()
            .get::<_, u32>(&StorageKey::Score(wallet.clone()))
            .unwrap_or_else(|| compute_score_from_metrics(&metrics));

        let tier = resolve_tier(score, &thresholds);

        Ok(InvestorScore {
            wallet,
            score,
            tier,
            metrics,
        })
    }
}
