#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{Pool, PoolComposition, StorageKey};

const BPS_DENOMINATOR: u32 = 10_000;
const MAX_POOL_INVOICES: u32 = 20;

#[contract]
pub struct InvoicePool;

fn load_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn load_pool(env: &Env, pool_id: u32) -> Result<Pool, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Pool(pool_id))
        .ok_or(Error::PoolNotFound)
}

#[contractimpl]
impl InvoicePool {
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::NextPoolId, &1u32);
        Ok(())
    }

    /// Admin-only. Weights are in bps and must sum to 10000.
    pub fn create_pool(
        env: Env,
        invoice_ids: Vec<BytesN<32>>,
        allocation_weights: Vec<u32>,
    ) -> Result<u32, Error> {
        load_admin(&env)?.require_auth();

        let count = invoice_ids.len();
        if count == 0 || count > MAX_POOL_INVOICES || count != allocation_weights.len() {
            return Err(Error::InvalidPool);
        }

        let mut total: u32 = 0;
        let mut allocations: Vec<i128> = Vec::new(&env);
        for i in 0..count {
            let weight = allocation_weights.get_unchecked(i);
            if weight == 0 {
                return Err(Error::InvalidWeights);
            }
            total = total.checked_add(weight).ok_or(Error::InvalidWeights)?;
            let id = invoice_ids.get_unchecked(i);
            for j in 0..i {
                if invoice_ids.get_unchecked(j) == id {
                    return Err(Error::InvalidPool);
                }
            }
            allocations.push_back(0);
        }
        if total != BPS_DENOMINATOR {
            return Err(Error::InvalidWeights);
        }

        let pool_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextPoolId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&StorageKey::NextPoolId, &(pool_id + 1));

        let pool = Pool {
            invoice_ids: invoice_ids.clone(),
            weights: allocation_weights.clone(),
            allocations,
            tvl: 0,
        };
        env.storage()
            .persistent()
            .set(&StorageKey::Pool(pool_id), &pool);

        events::pool_created(&env, pool_id, &invoice_ids, &allocation_weights);
        Ok(pool_id)
    }

    /// Admin-only. Sets the yield (bps) used for the weighted pool yield.
    pub fn set_invoice_yield(
        env: Env,
        invoice_id: BytesN<32>,
        yield_bps: u32,
    ) -> Result<(), Error> {
        load_admin(&env)?.require_auth();
        env.storage()
            .persistent()
            .set(&StorageKey::InvoiceYield(invoice_id), &yield_bps);
        Ok(())
    }

    /// Splits `amount` across the pool's invoices pro-rata by weight; any
    /// rounding remainder goes to the last invoice so allocations sum to `amount`.
    pub fn invest_pool(
        env: Env,
        investor: Address,
        pool_id: u32,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut pool = load_pool(&env, pool_id)?;
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let last = pool.weights.len() - 1;
        let mut distributed: i128 = 0;
        for i in 0..pool.weights.len() {
            let share = if i == last {
                amount - distributed
            } else {
                amount
                    .checked_mul(pool.weights.get_unchecked(i) as i128)
                    .ok_or(Error::Overflow)?
                    / BPS_DENOMINATOR as i128
            };
            distributed += share;
            let updated = pool
                .allocations
                .get_unchecked(i)
                .checked_add(share)
                .ok_or(Error::Overflow)?;
            pool.allocations.set(i, updated);
        }
        pool.tvl = pool.tvl.checked_add(amount).ok_or(Error::Overflow)?;

        token::Client::new(&env, &token_addr).transfer(
            &investor,
            &env.current_contract_address(),
            &amount,
        );

        env.storage()
            .persistent()
            .set(&StorageKey::Pool(pool_id), &pool);

        let position_key = StorageKey::Position(pool_id, investor.clone());
        let position: i128 = env.storage().persistent().get(&position_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&position_key, &(position + amount));

        events::pool_investment_executed(&env, pool_id, &investor, amount, &pool.allocations);
        Ok(())
    }

    pub fn get_pool_composition(env: Env, pool_id: u32) -> Result<PoolComposition, Error> {
        let pool = load_pool(&env, pool_id)?;
        Ok(PoolComposition {
            invoice_ids: pool.invoice_ids,
            weights: pool.weights,
            allocations: pool.allocations,
            tvl: pool.tvl,
        })
    }

    /// Weighted average (bps) of the constituent invoice yields.
    pub fn get_pool_yield(env: Env, pool_id: u32) -> Result<u32, Error> {
        let pool = load_pool(&env, pool_id)?;
        let mut weighted: u64 = 0;
        for i in 0..pool.invoice_ids.len() {
            let yield_bps: u32 = env
                .storage()
                .persistent()
                .get(&StorageKey::InvoiceYield(pool.invoice_ids.get_unchecked(i)))
                .ok_or(Error::YieldNotSet)?;
            weighted += yield_bps as u64 * pool.weights.get_unchecked(i) as u64;
        }
        Ok((weighted / BPS_DENOMINATOR as u64) as u32)
    }

    pub fn get_position(env: Env, pool_id: u32, investor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::Position(pool_id, investor))
            .unwrap_or(0)
    }
}
