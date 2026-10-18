#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Lock(Symbol, Address),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Lockup {
    pub unlock_at: u64,
    pub expired_emitted: bool,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidDuration = 3,
    TransferLocked = 4,
}

#[contract]
pub struct TransferRestriction;

#[contractimpl]
impl TransferRestriction {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn set_lockup(
        env: Env,
        invoice_id: Symbol,
        wallet: Address,
        duration_seconds: u64,
    ) -> Result<u64, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if duration_seconds == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::Lock(invoice_id, wallet));
            return Ok(env.ledger().timestamp());
        }
        let unlock_at = env
            .ledger()
            .timestamp()
            .checked_add(duration_seconds)
            .ok_or(Error::InvalidDuration)?;
        env.storage().persistent().set(
            &DataKey::Lock(invoice_id, wallet),
            &Lockup {
                unlock_at,
                expired_emitted: false,
            },
        );
        Ok(unlock_at)
    }

    pub fn is_transferable(env: Env, invoice_id: Symbol, wallet: Address) -> (bool, u64) {
        let lock: Option<Lockup> = env
            .storage()
            .persistent()
            .get(&DataKey::Lock(invoice_id, wallet));
        match lock {
            None => (true, 0),
            Some(lock) => (env.ledger().timestamp() >= lock.unlock_at, lock.unlock_at),
        }
    }

    pub fn validate_transfer(
        env: Env,
        invoice_id: Symbol,
        wallet: Address,
    ) -> Result<(), Error> {
        let key = DataKey::Lock(invoice_id.clone(), wallet.clone());
        let Some(mut lock) = env.storage().persistent().get::<_, Lockup>(&key) else {
            return Ok(());
        };
        if env.ledger().timestamp() < lock.unlock_at {
            return Err(Error::TransferLocked);
        }
        if !lock.expired_emitted {
            lock.expired_emitted = true;
            env.storage().persistent().set(&key, &lock);
            env.events().publish(
                (Symbol::new(&env, "LockupExpired"), invoice_id, wallet),
                lock.unlock_at,
            );
        }
        Ok(())
    }
}
