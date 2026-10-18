#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractclient, contractimpl, token, Address, BytesN, Env};
pub use types::{LoyaltyInfo, StorageKey, Tier};

/// Silver +0.25%, Gold +0.5%, Platinum +1% additional yield.
const SILVER_BOOST_BPS: u32 = 25;
const GOLD_BOOST_BPS: u32 = 50;
const PLATINUM_BOOST_BPS: u32 = 100;

#[contractclient(name = "LoyaltyClient")]
pub trait LoyaltyInterface {
    fn get_loyalty_tier(env: Env, wallet: Address) -> LoyaltyInfo;
}

#[contract]
pub struct YieldBooster;

fn boost_for_tier(tier: Tier) -> u32 {
    match tier {
        Tier::Bronze => 0,
        Tier::Silver => SILVER_BOOST_BPS,
        Tier::Gold => GOLD_BOOST_BPS,
        Tier::Platinum => PLATINUM_BOOST_BPS,
    }
}

fn current_tier(env: &Env, wallet: &Address) -> Result<Tier, Error> {
    let loyalty: Address = env
        .storage()
        .instance()
        .get(&StorageKey::LoyaltyContract)
        .ok_or(Error::NotInitialized)?;
    Ok(LoyaltyClient::new(env, &loyalty)
        .get_loyalty_tier(wallet)
        .tier)
}

#[contractimpl]
impl YieldBooster {
    pub fn initialize(
        env: Env,
        admin: Address,
        settlement: Address,
        loyalty_contract: Address,
        token: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::Settlement, &settlement);
        env.storage()
            .instance()
            .set(&StorageKey::LoyaltyContract, &loyalty_contract);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::BoostPool, &0i128);
        Ok(())
    }

    /// Admin-only: moves `amount` of the payment token into the boost pool.
    pub fn fund_boost_pool(env: Env, amount: i128) -> Result<i128, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;
        token::Client::new(&env, &token_addr).transfer(
            &admin,
            &env.current_contract_address(),
            &amount,
        );

        let balance: i128 = env
            .storage()
            .instance()
            .get(&StorageKey::BoostPool)
            .unwrap_or(0);
        let new_balance = balance.checked_add(amount).ok_or(Error::InvalidAmount)?;
        env.storage()
            .instance()
            .set(&StorageKey::BoostPool, &new_balance);

        events::boost_pool_funded(&env, &admin, amount, new_balance);
        Ok(new_balance)
    }

    pub fn get_boost_pool(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::BoostPool)
            .unwrap_or(0)
    }

    /// Called by the settlement contract. Records and returns the boost in
    /// bps for the wallet on this invoice; wallets below Silver get none.
    pub fn apply_boost(env: Env, wallet: Address, invoice_id: BytesN<32>) -> Result<u32, Error> {
        let settlement: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Settlement)
            .ok_or(Error::NotInitialized)?;
        settlement.require_auth();

        let key = StorageKey::Boost(wallet.clone(), invoice_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::BoostAlreadyApplied);
        }

        let tier = current_tier(&env, &wallet)?;
        let boost_bps = boost_for_tier(tier);
        if boost_bps == 0 {
            return Ok(0);
        }

        env.storage().persistent().set(&key, &boost_bps);
        events::yield_boost_applied(&env, &wallet, &invoice_id, tier, boost_bps);
        Ok(boost_bps)
    }

    /// Boost in bps for the wallet/invoice pair: the recorded value once
    /// applied, otherwise the boost the wallet's current tier would earn.
    pub fn get_boost_amount(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
    ) -> Result<u32, Error> {
        if let Some(applied) = env
            .storage()
            .persistent()
            .get::<_, u32>(&StorageKey::Boost(wallet.clone(), invoice_id))
        {
            return Ok(applied);
        }
        Ok(boost_for_tier(current_tier(&env, &wallet)?))
    }
}
