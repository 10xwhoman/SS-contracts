#![allow(deprecated)]
//! Event definitions for the reputation contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::ReputationScore;

/// Emitted whenever a settlement is recorded and the score is updated.
/// Carries the issuer address and the new score.
pub fn reputation_updated(env: &Env, issuer: &Address, score: &ReputationScore) {
    env.events().publish(
        (Symbol::new(env, "reputation_updated"),),
        (
            issuer,
            score.score,
            score.components.on_time_rate,
            score.components.avg_days_variance,
            score.components.settlement_count,
        ),
    );
}
