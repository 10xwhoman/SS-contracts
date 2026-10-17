#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

const MAX_RECIPIENTS: u32 = 32;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Config,
    Balance,
    Epoch,
    Distribution(u64),
}

#[derive(Clone)]
#[contracttype]
struct Config {
    admin: Address,
    token: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DistributionRecord {
    pub epoch: u64,
    pub recipients: Vec<Address>,
    pub amounts: Vec<i128>,
    pub total: i128,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidRecipients = 5,
    InsufficientBalance = 6,
    Overflow = 7,
}

#[contract]
pub struct FeeTreasury;

#[contractimpl]
impl FeeTreasury {
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { admin, token });
        env.storage().instance().set(&DataKey::Balance, &0i128);
        env.storage().instance().set(&DataKey::Epoch, &0u64);
        Ok(())
    }

    pub fn collect_fee(env: Env, source: Address, amount: i128) -> Result<(), Error> {
        source.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(Error::NotInitialized)?;
        soroban_sdk::token::Client::new(&env, &config.token).transfer(
            &source,
            &env.current_contract_address(),
            &amount,
        );
        let balance: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
        env.storage().instance().set(
            &DataKey::Balance,
            &balance.checked_add(amount).ok_or(Error::Overflow)?,
        );
        env.events()
            .publish((Symbol::new(&env, "FeeCollected"), source), amount);
        Ok(())
    }

    pub fn get_balance(env: Env) -> Result<i128, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Balance)
            .ok_or(Error::NotInitialized)
    }

    pub fn distribute(
        env: Env,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
    ) -> Result<u64, Error> {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(Error::NotInitialized)?;
        config.admin.require_auth();
        if recipients.is_empty()
            || recipients.len() != amounts.len()
            || recipients.len() > MAX_RECIPIENTS
        {
            return Err(Error::InvalidRecipients);
        }
        let mut total = 0i128;
        for amount in amounts.iter() {
            if amount <= 0 {
                return Err(Error::InvalidAmount);
            }
            total = total.checked_add(amount).ok_or(Error::Overflow)?;
        }
        let balance: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
        if total > balance {
            return Err(Error::InsufficientBalance);
        }
        let token = soroban_sdk::token::Client::new(&env, &config.token);
        for i in 0..recipients.len() {
            token.transfer(
                &env.current_contract_address(),
                &recipients.get(i).unwrap(),
                &amounts.get(i).unwrap(),
            );
        }
        let epoch: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Epoch)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::Epoch, &epoch);
        env.storage()
            .instance()
            .set(&DataKey::Balance, &(balance - total));
        let record = DistributionRecord {
            epoch,
            recipients: recipients.clone(),
            amounts: amounts.clone(),
            total,
        };
        env.storage()
            .instance()
            .set(&DataKey::Distribution(epoch), &record);
        env.events().publish(
            (Symbol::new(&env, "FeeDistributed"), epoch),
            (recipients, amounts, total),
        );
        Ok(epoch)
    }

    pub fn get_distribution(env: Env, epoch: u64) -> Result<DistributionRecord, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Distribution(epoch))
            .ok_or(Error::NotInitialized)
    }
}
