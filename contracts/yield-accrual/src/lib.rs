#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
};

const BPS: i128 = 10_000;
const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Clone)]
#[contracttype]
pub struct Position {
    pub principal: i128,
    pub rate_bps: u32,
    pub start_ts: u64,
    pub tenor_days: u64,
    pub accrued: i128,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Authorized(Address),
    Position(BytesN<32>, Address),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidPosition = 4,
    ArithmeticOverflow = 5,
}

#[contract]
pub struct YieldAccrual;

fn authorized(env: &Env, caller: &Address) -> Result<(), Error> {
    if !env
        .storage()
        .persistent()
        .get(&DataKey::Authorized(caller.clone()))
        .unwrap_or(false)
    {
        return Err(Error::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}
fn calculate(position: &Position, now: u64) -> Result<i128, Error> {
    if now <= position.start_ts || position.tenor_days == 0 {
        return Ok(0);
    }
    let elapsed = now
        .saturating_sub(position.start_ts)
        .min(position.tenor_days.saturating_mul(SECONDS_PER_DAY));
    position
        .principal
        .checked_mul(position.rate_bps as i128)
        .ok_or(Error::ArithmeticOverflow)
        .and_then(|v| {
            v.checked_mul(elapsed as i128)
                .ok_or(Error::ArithmeticOverflow)
        })
        .and_then(|v| {
            v.checked_div(BPS * position.tenor_days as i128 * SECONDS_PER_DAY)
                .ok_or(Error::ArithmeticOverflow)
        })
}

#[contractimpl]
impl YieldAccrual {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Authorized(admin), &true);
        Ok(())
    }
    pub fn set_authorized_caller(env: Env, caller: Address, allowed: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Authorized(caller), &allowed);
        Ok(())
    }
    pub fn set_position(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        investor: Address,
        principal: i128,
        rate_bps: u32,
        start_ts: u64,
        tenor_days: u64,
    ) -> Result<(), Error> {
        authorized(&env, &caller)?;
        if principal <= 0 || rate_bps > 100_000 || tenor_days == 0 {
            return Err(Error::InvalidPosition);
        }
        env.storage().persistent().set(
            &DataKey::Position(invoice_id, investor),
            &Position {
                principal,
                rate_bps,
                start_ts,
                tenor_days,
                accrued: 0,
            },
        );
        Ok(())
    }
    pub fn accrue_yield(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<i128, Error> {
        authorized(&env, &caller)?;
        let key = DataKey::Position(invoice_id.clone(), investor.clone());
        let mut position: Position = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::InvalidPosition)?;
        let earned = calculate(&position, env.ledger().timestamp())?;
        let delta = earned.saturating_sub(position.accrued);
        position.accrued = earned;
        env.storage().persistent().set(&key, &position);
        env.events()
            .publish((symbol_short!("yield"), invoice_id, investor), earned);
        Ok(delta)
    }
    pub fn get_accrued_yield(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
    ) -> Result<i128, Error> {
        let position: Position = env
            .storage()
            .persistent()
            .get(&DataKey::Position(invoice_id, investor))
            .ok_or(Error::InvalidPosition)?;
        calculate(&position, env.ledger().timestamp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    #[test]
    fn accrues_pro_rata_yield() {
        let env = Env::default();
        env.ledger().set(Ledger {
            timestamp: 1_000,
            ..Default::default()
        });
        let contract = env.register(YieldAccrual, ());
        let client = YieldAccrualClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let invoice = BytesN::from_array(&env, &[1; 32]);
        let investor = Address::generate(&env);
        client.initialize(&admin);
        client.set_position(&admin, &invoice, &investor, &10_000, &1_000, &1_000, &10);
        assert_eq!(client.get_accrued_yield(&invoice, &investor), 0);
        env.ledger().set(Ledger {
            timestamp: 1_000 + 5 * SECONDS_PER_DAY,
            ..Default::default()
        });
        assert_eq!(client.get_accrued_yield(&invoice, &investor), 500);
    }
}
