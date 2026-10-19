//! Fee discount contract for StellarSettle (issue #621).
//!
//! Applies reduced platform fees to investors based on their loyalty tier,
//! incentivising continued platform activity with tiered cost reductions.
//!
//! Discount schedule (off the platform fee):
//!   Bronze   –  0 % (0 bps)
//!   Silver   –  5 % (500 bps)
//!   Gold     – 10 % (1000 bps)
//!   Platinum – 20 % (2000 bps)

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{Config, LoyaltyTier, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, Env};

const BPS_DENOMINATOR: i128 = 10_000;

/// Discount bps per tier (as a fraction of the platform fee).
///   Bronze   →   0 bps  (no discount)
///   Silver   → 500 bps  (5 % off)
///   Gold     → 1000 bps (10 % off)
///   Platinum → 2000 bps (20 % off)
fn discount_bps_for_tier(tier: LoyaltyTier) -> i128 {
    match tier {
        LoyaltyTier::Bronze => 0,
        LoyaltyTier::Silver => 500,
        LoyaltyTier::Gold => 1_000,
        LoyaltyTier::Platinum => 2_000,
    }
}

fn load_config(env: &Env) -> Result<Config, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .ok_or(Error::NotInitialized)
}

fn get_wallet_tier(env: &Env, wallet: &Address) -> LoyaltyTier {
    env.storage()
        .persistent()
        .get(&StorageKey::WalletTier(wallet.clone()))
        .unwrap_or(LoyaltyTier::Bronze)
}

/// Compute the fee after applying the discount for `wallet`'s loyalty tier.
/// Returns `(discount_amount, final_fee)`.
fn apply_discount(
    env: &Env,
    wallet: &Address,
    original_fee: i128,
) -> Result<(LoyaltyTier, i128, i128), Error> {
    let tier = get_wallet_tier(env, wallet);
    let bps = discount_bps_for_tier(tier);
    let discount = original_fee
        .checked_mul(bps)
        .ok_or(Error::Overflow)?
        / BPS_DENOMINATOR;
    let final_fee = original_fee
        .checked_sub(discount)
        .ok_or(Error::Overflow)?;
    Ok((tier, discount, final_fee))
}

#[contract]
pub struct FeeDiscount;

#[contractimpl]
impl FeeDiscount {
    /// Initialise the contract.
    ///
    /// * `admin`            – address allowed to update config.
    /// * `loyalty_contract` – address of the platform loyalty-tier contract.
    /// * `platform_fee_bps` – base platform fee in basis points (e.g. 200 = 2 %).
    pub fn initialize(
        env: Env,
        admin: Address,
        loyalty_contract: Address,
        platform_fee_bps: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        if platform_fee_bps > 10_000 {
            return Err(Error::InvalidFeeBps);
        }
        env.storage().instance().set(
            &StorageKey::Config,
            &Config {
                admin,
                loyalty_contract,
                platform_fee_bps,
            },
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin helpers
    // -----------------------------------------------------------------------

    /// Admin-only: set or update the loyalty tier for a wallet directly.
    /// This is the on-chain injection point used by the platform; in
    /// production the loyalty contract would call this after every upgrade.
    pub fn set_wallet_tier(env: Env, wallet: Address, tier: LoyaltyTier) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        cfg.admin.require_auth();
        env.storage()
            .persistent()
            .set(&StorageKey::WalletTier(wallet), &tier);
        Ok(())
    }

    /// Admin-only: update the base platform fee.
    pub fn set_platform_fee_bps(env: Env, new_bps: u32) -> Result<(), Error> {
        let mut cfg = load_config(&env)?;
        cfg.admin.require_auth();
        if new_bps > 10_000 {
            return Err(Error::InvalidFeeBps);
        }
        cfg.platform_fee_bps = new_bps;
        env.storage()
            .instance()
            .set(&StorageKey::Config, &cfg);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Public read helpers
    // -----------------------------------------------------------------------

    /// Returns the discount in basis points for `wallet`'s current loyalty tier.
    pub fn get_fee_discount(env: Env, wallet: Address) -> Result<i128, Error> {
        load_config(&env)?; // ensure initialised
        let tier = get_wallet_tier(&env, &wallet);
        Ok(discount_bps_for_tier(tier))
    }

    /// Returns the wallet's current loyalty tier.
    pub fn get_wallet_tier(env: Env, wallet: Address) -> Result<LoyaltyTier, Error> {
        load_config(&env)?;
        Ok(get_wallet_tier(&env, &wallet))
    }

    // -----------------------------------------------------------------------
    // Fee-application entry points
    // -----------------------------------------------------------------------

    /// Calculate and apply the fee discount for an investment transaction.
    ///
    /// `investment_amount` is the gross investment. The function derives the
    /// original platform fee from `platform_fee_bps`, reduces it by the
    /// wallet's tier discount, emits `FeeDiscountApplied`, and returns the
    /// final (post-discount) fee.
    pub fn invest(env: Env, wallet: Address, investment_amount: i128) -> Result<i128, Error> {
        wallet.require_auth();
        let cfg = load_config(&env)?;
        if investment_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let original_fee = investment_amount
            .checked_mul(cfg.platform_fee_bps as i128)
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;

        let (tier, discount_amount, final_fee) = apply_discount(&env, &wallet, original_fee)?;

        events::fee_discount_applied(
            &env,
            &wallet,
            tier,
            original_fee,
            discount_amount,
            final_fee,
        );

        Ok(final_fee)
    }

    /// Calculate and apply the fee discount for a secondary-market buy.
    ///
    /// Semantics are identical to `invest`; kept as a separate entry point so
    /// the platform can distinguish the two operation types in event logs.
    pub fn buy(env: Env, wallet: Address, purchase_amount: i128) -> Result<i128, Error> {
        wallet.require_auth();
        let cfg = load_config(&env)?;
        if purchase_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let original_fee = purchase_amount
            .checked_mul(cfg.platform_fee_bps as i128)
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;

        let (tier, discount_amount, final_fee) = apply_discount(&env, &wallet, original_fee)?;

        events::fee_discount_applied(
            &env,
            &wallet,
            tier,
            original_fee,
            discount_amount,
            final_fee,
        );

        Ok(final_fee)
    }
}
