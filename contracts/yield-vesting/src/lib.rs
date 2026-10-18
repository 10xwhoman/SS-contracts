#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{StorageKey, Tranche, VestSchedule};

#[contract]
pub struct YieldVesting;

#[contractimpl]
impl YieldVesting {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::NextVestId, &1u32);
        Ok(())
    }

    pub fn set_authorized(env: Env, caller: Address, authorized: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&StorageKey::Authorized(caller), &authorized);
        Ok(())
    }

    pub fn create_yield_vest(
        env: Env,
        invoice_id: BytesN<32>,
        beneficiary: Address,
        token: Address,
        tranches: Vec<Tranche>,
    ) -> Result<u32, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        if tranches.is_empty() {
            return Err(Error::EmptyTranches);
        }

        let mut total_amount: i128 = 0;
        for i in 0..tranches.len() {
            let t = tranches.get(i).unwrap();
            if t.amount <= 0 {
                return Err(Error::InvalidAmount);
            }
            total_amount = total_amount
                .checked_add(t.amount)
                .ok_or(Error::InvalidAmount)?;
        }

        let vest_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextVestId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&StorageKey::NextVestId, &(vest_id + 1));

        let schedule = VestSchedule {
            vest_id,
            invoice_id: invoice_id.clone(),
            beneficiary: beneficiary.clone(),
            token: token.clone(),
            total_amount,
            claimed_amount: 0,
            tranches: tranches.clone(),
        };

        env.storage().persistent().set(&StorageKey::Vest(vest_id), &schedule);

        events::vest_created(
            &env,
            vest_id,
            &invoice_id,
            &beneficiary,
            total_amount,
            tranches.len(),
        );

        Ok(vest_id)
    }

    pub fn claim_tranche(env: Env, vest_id: u32, tranche_index: u32) -> Result<i128, Error> {
        let mut schedule: VestSchedule = env
            .storage()
            .persistent()
            .get(&StorageKey::Vest(vest_id))
            .ok_or(Error::VestNotFound)?;

        schedule.beneficiary.require_auth();

        if tranche_index >= schedule.tranches.len() {
            return Err(Error::InvalidTrancheIndex);
        }

        let mut tranche = schedule.tranches.get(tranche_index).unwrap();
        if tranche.claimed {
            return Err(Error::AlreadyClaimed);
        }

        let current_time = env.ledger().timestamp();
        if current_time < tranche.release_timestamp {
            return Err(Error::TimeNotReached);
        }

        tranche.claimed = true;
        schedule.tranches.set(tranche_index, tranche.clone());
        schedule.claimed_amount = schedule
            .claimed_amount
            .checked_add(tranche.amount)
            .ok_or(Error::InvalidAmount)?;

        env.storage().persistent().set(&StorageKey::Vest(vest_id), &schedule);

        events::tranche_released(
            &env,
            vest_id,
            tranche_index,
            &schedule.beneficiary,
            tranche.amount,
        );

        let client = token::Client::new(&env, &schedule.token);
        let contract_balance = client.balance(&env.current_contract_address());
        if contract_balance >= tranche.amount {
            client.transfer(&env.current_contract_address(), &schedule.beneficiary, &tranche.amount);
        }

        Ok(tranche.amount)
    }

    pub fn claim_batch_tranches(
        env: Env,
        vest_id: u32,
        tranche_indices: Vec<u32>,
    ) -> Result<i128, Error> {
        let mut schedule: VestSchedule = env
            .storage()
            .persistent()
            .get(&StorageKey::Vest(vest_id))
            .ok_or(Error::VestNotFound)?;

        schedule.beneficiary.require_auth();

        let current_time = env.ledger().timestamp();
        let mut total_claimed: i128 = 0;

        for i in 0..tranche_indices.len() {
            let idx = tranche_indices.get(i).unwrap();
            if idx >= schedule.tranches.len() {
                return Err(Error::InvalidTrancheIndex);
            }
            let mut tranche = schedule.tranches.get(idx).unwrap();
            if tranche.claimed {
                return Err(Error::AlreadyClaimed);
            }
            if current_time < tranche.release_timestamp {
                return Err(Error::TimeNotReached);
            }

            tranche.claimed = true;
            schedule.tranches.set(idx, tranche.clone());
            total_claimed = total_claimed
                .checked_add(tranche.amount)
                .ok_or(Error::InvalidAmount)?;

            events::tranche_released(
                &env,
                vest_id,
                idx,
                &schedule.beneficiary,
                tranche.amount,
            );
        }

        schedule.claimed_amount = schedule
            .claimed_amount
            .checked_add(total_claimed)
            .ok_or(Error::InvalidAmount)?;

        env.storage().persistent().set(&StorageKey::Vest(vest_id), &schedule);

        let client = token::Client::new(&env, &schedule.token);
        let contract_balance = client.balance(&env.current_contract_address());
        if contract_balance >= total_claimed {
            client.transfer(&env.current_contract_address(), &schedule.beneficiary, &total_claimed);
        }

        Ok(total_claimed)
    }

    pub fn claim_all_eligible_tranches(env: Env, vest_id: u32) -> Result<i128, Error> {
        let mut schedule: VestSchedule = env
            .storage()
            .persistent()
            .get(&StorageKey::Vest(vest_id))
            .ok_or(Error::VestNotFound)?;

        schedule.beneficiary.require_auth();

        let current_time = env.ledger().timestamp();
        let mut total_claimed: i128 = 0;
        let mut claimed_any = false;

        for i in 0..schedule.tranches.len() {
            let mut tranche = schedule.tranches.get(i).unwrap();
            if !tranche.claimed && current_time >= tranche.release_timestamp {
                tranche.claimed = true;
                schedule.tranches.set(i, tranche.clone());
                total_claimed = total_claimed
                    .checked_add(tranche.amount)
                    .ok_or(Error::InvalidAmount)?;
                claimed_any = true;

                events::tranche_released(
                    &env,
                    vest_id,
                    i,
                    &schedule.beneficiary,
                    tranche.amount,
                );
            }
        }

        if !claimed_any {
            return Err(Error::NoEligibleTranches);
        }

        schedule.claimed_amount = schedule
            .claimed_amount
            .checked_add(total_claimed)
            .ok_or(Error::InvalidAmount)?;

        env.storage().persistent().set(&StorageKey::Vest(vest_id), &schedule);

        let client = token::Client::new(&env, &schedule.token);
        let contract_balance = client.balance(&env.current_contract_address());
        if contract_balance >= total_claimed {
            client.transfer(&env.current_contract_address(), &schedule.beneficiary, &total_claimed);
        }

        Ok(total_claimed)
    }

    pub fn get_vest_schedule(env: Env, vest_id: u32) -> Result<VestSchedule, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Vest(vest_id))
            .ok_or(Error::VestNotFound)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)
    }
}
