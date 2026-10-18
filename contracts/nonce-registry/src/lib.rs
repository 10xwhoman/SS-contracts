#![no_std]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec, U256,
};

/// Upper bound on nonces burned in one `bulk_invalidate` call.
pub const MAX_BULK_INVALIDATE: u32 = 50;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NonceAlreadyUsed = 3,
    BatchTooLarge = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Consumer,
    Used(Address, U256),
}

#[contract]
pub struct NonceRegistry;

#[contractimpl]
impl NonceRegistry {
    /// `consumer` is the contract that verifies signed messages and spends nonces.
    pub fn initialize(env: Env, consumer: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Consumer) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Consumer, &consumer);
        Ok(())
    }

    /// Marks `nonce` as used for `wallet`; fails if it was already consumed.
    pub fn consume_nonce(env: Env, wallet: Address, nonce: U256) -> Result<(), Error> {
        let consumer: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Consumer)
            .ok_or(Error::NotInitialized)?;
        consumer.require_auth();

        if Self::is_nonce_used(env.clone(), wallet.clone(), nonce.clone()) {
            return Err(Error::NonceAlreadyUsed);
        }
        Self::mark_used(&env, wallet, nonce);
        Ok(())
    }

    pub fn is_nonce_used(env: Env, wallet: Address, nonce: U256) -> bool {
        env.storage()
            .persistent()
            .has(&StorageKey::Used(wallet, nonce))
    }

    /// Lets a wallet pre-emptively burn nonces it no longer wants honoured.
    /// Nonces that are already used are skipped rather than rejected.
    pub fn bulk_invalidate(env: Env, wallet: Address, nonces: Vec<U256>) -> Result<(), Error> {
        wallet.require_auth();
        if nonces.len() > MAX_BULK_INVALIDATE {
            return Err(Error::BatchTooLarge);
        }
        for nonce in nonces.iter() {
            if !Self::is_nonce_used(env.clone(), wallet.clone(), nonce.clone()) {
                Self::mark_used(&env, wallet.clone(), nonce);
            }
        }
        Ok(())
    }
}

impl NonceRegistry {
    #[allow(deprecated)]
    fn mark_used(env: &Env, wallet: Address, nonce: U256) {
        env.storage()
            .persistent()
            .set(&StorageKey::Used(wallet.clone(), nonce.clone()), &true);
        let topics = (Symbol::new(env, "NonceConsumed"), wallet);
        env.events().publish(topics, nonce);
    }
}
