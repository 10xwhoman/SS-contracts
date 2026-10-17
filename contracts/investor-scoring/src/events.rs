use crate::types::Tier;
use soroban_sdk::{Address, Env, Symbol};

pub fn score_updated(env: &Env, wallet: &Address, old_score: u32, new_score: u32, tier: Tier) {
    let topics = (Symbol::new(env, "ScoreUpdated"), wallet.clone());
    env.events()
        .publish(topics, (old_score, new_score, tier as u32));
}

pub fn tier_thresholds_updated(env: &Env, silver: u32, gold: u32, platinum: u32) {
    let topics = (Symbol::new(env, "TierThresholdsUpdated"),);
    env.events().publish(topics, (silver, gold, platinum));
}

pub fn authorized_caller_updated(env: &Env, caller: &Address, authorized: bool) {
    let topics = (Symbol::new(env, "AuthorizedCallerUpdated"), caller.clone());
    env.events().publish(topics, authorized);
}
