//! Reputation contract for StellarSettle.
//!
//! Records issuer settlement outcomes and computes a reputation score
//! (0–100) from on-time rate and days variance, stored immutably on-chain
//! for investor due diligence.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

use errors::Error;
use types::{ReputationScore, ScoreComponents, SettlementRecord};

#[contract]
pub struct Reputation;

/// Compute the reputation score from a list of settlement records.
///
/// The score combines two components:
/// - **On-time rate** (60% weight): percentage of settlements where
///   `days_offset >= 0` (settled on or before the due date).
/// - **Variance score** (40% weight): `max(0, 100 - avg_days_late * 2)`,
///   where `avg_days_late` is the average number of days late across all
///   settlements (early settlements contribute zero). Each day late reduces
///   the score by 2 points.
///
/// Returns a `ReputationScore` with the total (0–100) and component breakdown.
fn compute_score(records: &Vec<SettlementRecord>) -> ReputationScore {
    let count = records.len();

    if count == 0 {
        return ReputationScore {
            score: 0,
            components: ScoreComponents {
                on_time_rate: 0,
                avg_days_variance: 0,
                settlement_count: 0,
            },
        };
    }

    let count_i128 = i128::from(count);

    // On-time count: settlements where days_offset >= 0
    let mut on_time: i128 = 0;
    let mut total_days_late: i128 = 0;

    for record in records.iter() {
        if record.days_offset >= 0 {
            on_time += 1;
        } else {
            // Only lateness is penalized; settling early is never a penalty.
            total_days_late += i128::from(-record.days_offset);
        }
    }

    // On-time rate as 0-100 (integer percentage).
    let on_time_rate_bps = on_time
        .checked_mul(10_000)
        .unwrap_or(0)
        .checked_div(count_i128)
        .unwrap_or(0);
    let on_time_rate = u32::try_from(on_time_rate_bps.checked_div(100).unwrap_or(0)).unwrap_or(0);

    // Average days late across all settlements.
    let avg_variance = total_days_late.checked_div(count_i128).unwrap_or(0);
    let avg_days_variance = i32::try_from(avg_variance).unwrap_or(0);

    // Variance score: each day late reduces score by 2 points, clamped to 0.
    let variance_score: i128 = {
        let penalty = avg_variance.checked_mul(2).unwrap_or(i128::MAX);
        if penalty >= 100 {
            0
        } else {
            100 - penalty
        }
    };

    // Weighted total: on_time_rate * 0.6 + variance_score * 0.4.
    // Computed in basis points to avoid floating point: total_bps = on_time_bps * 60/100 + var_bps * 40/100.
    let on_time_bps = on_time_rate_bps; // already in 0-10000
    let variance_bps = variance_score.checked_mul(100).unwrap_or(0); // 0-10000

    let total_bps = on_time_bps
        .checked_mul(60)
        .unwrap_or(0)
        .checked_div(100)
        .unwrap_or(0)
        .checked_add(
            variance_bps
                .checked_mul(40)
                .unwrap_or(0)
                .checked_div(100)
                .unwrap_or(0),
        )
        .unwrap_or(0);

    let score = u32::try_from(total_bps.checked_div(100).unwrap_or(0)).unwrap_or(0);
    let score = score.min(100);

    ReputationScore {
        score,
        components: ScoreComponents {
            on_time_rate,
            avg_days_variance,
            settlement_count: count,
        },
    }
}

#[contractimpl]
impl Reputation {
    /// Initialize the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        Ok(())
    }

    /// Record a settlement outcome for an issuer. Admin-only.
    /// `days_early_or_late`: positive = early, negative = late, zero = on time.
    /// Emits a `ReputationUpdated` event with the new score.
    pub fn record_settlement(
        env: Env,
        issuer: Address,
        invoice_id: Symbol,
        days_early_or_late: i32,
    ) -> Result<(), Error> {
        let admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        admin.require_auth();

        let mut records = storage::get_settlements(&env, &issuer);
        records.push_back(SettlementRecord {
            invoice_id,
            days_offset: days_early_or_late,
            timestamp: env.ledger().timestamp(),
        });
        storage::set_settlements(&env, &issuer, &records);

        let score = compute_score(&records);
        events::reputation_updated(&env, &issuer, &score);
        Ok(())
    }

    /// View: return the reputation score (0–100) with component breakdown.
    pub fn get_score(env: Env, issuer: Address) -> Result<ReputationScore, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        let records = storage::get_settlements(&env, &issuer);
        Ok(compute_score(&records))
    }

    /// View: return all recorded settlement outcomes for an issuer, in order.
    pub fn get_settlement_history(
        env: Env,
        issuer: Address,
    ) -> Result<Vec<SettlementRecord>, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_settlements(&env, &issuer))
    }
}

#[cfg(test)]
mod test;
