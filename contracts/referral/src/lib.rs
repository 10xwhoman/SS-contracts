#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Issue #585: Referral reward contract for investor acquisition incentives
/// 
/// Rewards investors for bringing new wallets to the platform, distributing
/// 20% of the platform fee on the referred wallet's first investment.

const REFERRAL_REWARD_BPS: i128 = 2000; // 20% in basis points
const BPS_DENOMINATOR: i128 = 10000;

#[contracttype]
#[derive(Clone)]
pub struct ReferralStats {
    pub referred_count: u32,
    pub total_earnings: i128,
}

#[contracttype]
pub enum DataKey {
    Referrer(Address), // wallet -> referrer address
    FirstInvestment(Address), // wallet -> bool (has made first investment)
    Stats(Address), // referrer -> ReferralStats
}

#[contract]
pub struct ReferralContract;

#[contractimpl]
impl ReferralContract {
    /// Register a referral link between wallet and referrer
    pub fn register_referral(env: Env, wallet: Address, referrer: Address) {
        wallet.require_auth();

        // Reject self-referral
        if wallet == referrer {
            panic!("Self-referral not allowed");
        }

        // Only register if wallet hasn't been referred yet
        if env.storage().persistent().has(&DataKey::Referrer(wallet.clone())) {
            panic!("Wallet already referred");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Referrer(wallet.clone()), &referrer);

        // Initialize first investment flag
        env.storage()
            .persistent()
            .set(&DataKey::FirstInvestment(wallet.clone()), &false);

        // Update referrer stats
        let mut stats = Self::get_referral_stats(env.clone(), referrer.clone());
        stats.referred_count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Stats(referrer.clone()), &stats);

        env.events().publish(
            (Symbol::new(&env, "ReferralRegistered"), wallet.clone()),
            referrer,
        );
    }

    /// Process first investment and pay referral reward
    pub fn on_first_investment(
        env: Env,
        wallet: Address,
        fee_amount: i128,
    ) -> Option<i128> {
        // Check if this is the first investment
        let is_first: bool = env
            .storage()
            .persistent()
            .get(&DataKey::FirstInvestment(wallet.clone()))
            .unwrap_or(true);

        if !is_first {
            // Not first investment, no reward
            return None;
        }

        // Mark as invested
        env.storage()
            .persistent()
            .set(&DataKey::FirstInvestment(wallet.clone()), &true);

        // Check if wallet has a referrer
        let referrer: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Referrer(wallet.clone()));

        if let Some(referrer_address) = referrer {
            // Calculate referral reward: 20% of platform fee
            let reward = (fee_amount * REFERRAL_REWARD_BPS) / BPS_DENOMINATOR;

            // Update referrer stats
            let mut stats = Self::get_referral_stats(env.clone(), referrer_address.clone());
            stats.total_earnings += reward;
            env.storage()
                .persistent()
                .set(&DataKey::Stats(referrer_address.clone()), &stats);

            // Emit ReferralRewardPaid event
            env.events().publish(
                (Symbol::new(&env, "ReferralRewardPaid"), referrer_address.clone()),
                (wallet, reward),
            );

            Some(reward)
        } else {
            // No referrer, no reward
            None
        }
    }

    /// Get referral statistics for a referrer
    pub fn get_referral_stats(env: Env, referrer: Address) -> ReferralStats {
        env.storage()
            .persistent()
            .get(&DataKey::Stats(referrer))
            .unwrap_or(ReferralStats {
                referred_count: 0,
                total_earnings: 0,
            })
    }

    /// Check if a wallet has a referrer
    pub fn get_referrer(env: Env, wallet: Address) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Referrer(wallet))
    }

    /// Check if wallet has made first investment
    pub fn has_invested(env: Env, wallet: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::FirstInvestment(wallet))
            .unwrap_or(false)
    }

    /// Calculate potential reward for a fee amount
    pub fn calculate_reward(fee_amount: i128) -> i128 {
        (fee_amount * REFERRAL_REWARD_BPS) / BPS_DENOMINATOR
    }
}
