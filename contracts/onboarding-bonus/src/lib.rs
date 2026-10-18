#![no_std]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Symbol,
};

/// 0.5% additional yield credited on the first investment.
const BONUS_BPS: i128 = 50;
const BPS_DENOMINATOR: i128 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    AlreadyClaimed = 4,
    InsufficientBonusPool = 5,
    Overflow = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BonusStatus {
    pub claimed: bool,
    pub bonus_amount: i128,
    pub invoice_id: Option<BytesN<32>>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Token,
    Escrow,
    BonusPool,
    Claim(Address),
}

#[contract]
pub struct OnboardingBonus;

#[contractimpl]
impl OnboardingBonus {
    /// `escrow` is the investment contract that reports first investments.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        escrow: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::Escrow, &escrow);
        Ok(())
    }

    /// Moves `amount` from the admin (treasury) account into the bonus pool.
    pub fn fund_bonus_pool(env: Env, amount: i128) -> Result<(), Error> {
        let admin = Self::address(&env, &StorageKey::Admin)?;
        admin.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let pool = Self::pool(&env)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        let token = Self::address(&env, &StorageKey::Token)?;
        token::Client::new(&env, &token).transfer(
            &admin,
            &env.current_contract_address(),
            &amount,
        );
        env.storage().instance().set(&StorageKey::BonusPool, &pool);
        Ok(())
    }

    /// Pays 0.5% of `investment_amount` to `wallet`; only the first investment per wallet qualifies.
    #[allow(deprecated)]
    pub fn claim_onboarding_bonus(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
        investment_amount: i128,
    ) -> Result<i128, Error> {
        let escrow = Self::address(&env, &StorageKey::Escrow)?;
        escrow.require_auth();
        if investment_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let key = StorageKey::Claim(wallet.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyClaimed);
        }
        let bonus = investment_amount
            .checked_mul(BONUS_BPS)
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;
        let pool = Self::pool(&env);
        if bonus > pool {
            return Err(Error::InsufficientBonusPool);
        }

        env.storage().persistent().set(
            &key,
            &BonusStatus {
                claimed: true,
                bonus_amount: bonus,
                invoice_id: Some(invoice_id.clone()),
            },
        );
        env.storage()
            .instance()
            .set(&StorageKey::BonusPool, &(pool - bonus));
        if bonus > 0 {
            let token = Self::address(&env, &StorageKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &wallet,
                &bonus,
            );
        }

        let topics = (Symbol::new(&env, "OnboardingBonusClaimed"), wallet);
        env.events().publish(topics, (invoice_id, bonus));
        Ok(bonus)
    }

    pub fn get_bonus_status(env: Env, wallet: Address) -> BonusStatus {
        env.storage()
            .persistent()
            .get(&StorageKey::Claim(wallet))
            .unwrap_or(BonusStatus {
                claimed: false,
                bonus_amount: 0,
                invoice_id: None,
            })
    }

    pub fn get_bonus_pool(env: Env) -> i128 {
        Self::pool(&env)
    }
}

impl OnboardingBonus {
    fn address(env: &Env, key: &StorageKey) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(key)
            .ok_or(Error::NotInitialized)
    }

    fn pool(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::BonusPool)
            .unwrap_or(0)
    }
}
