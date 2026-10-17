#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
};

const DEFAULT_LIMIT_BPS: u32 = 2_000;
const MAX_BPS: u32 = 10_000;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Limit(Address),
    Total(Address),
    Position(Address, BytesN<32>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    InvalidLimit = 2,
    ConcentrationLimitExceeded = 3,
    ArithmeticOverflow = 4,
}

#[contract]
pub struct InvestmentLimit;

fn limit(env: &Env, investor: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Limit(investor.clone()))
        .unwrap_or(DEFAULT_LIMIT_BPS)
}
fn position(env: &Env, investor: &Address, invoice: &BytesN<32>) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Position(investor.clone(), invoice.clone()))
        .unwrap_or(0)
}
fn total(env: &Env, investor: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Total(investor.clone()))
        .unwrap_or(0)
}

#[contractimpl]
impl InvestmentLimit {
    pub fn set_concentration_limit(
        env: Env,
        investor: Address,
        limit_bps: u32,
    ) -> Result<(), Error> {
        investor.require_auth();
        if limit_bps == 0 || limit_bps > MAX_BPS {
            return Err(Error::InvalidLimit);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Limit(investor), &limit_bps);
        Ok(())
    }
    pub fn get_concentration_limit(env: Env, investor: Address) -> u32 {
        limit(&env, &investor)
    }
    pub fn check_concentration(
        env: Env,
        investor: Address,
        invoice: BytesN<32>,
        new_amount: i128,
    ) -> Result<bool, Error> {
        if new_amount < 0 {
            return Err(Error::InvalidAmount);
        }
        let current_total = total(&env, &investor);
        let projected_total = current_total
            .checked_add(new_amount)
            .ok_or(Error::ArithmeticOverflow)?;
        if projected_total == 0 {
            return Ok(true);
        }
        let projected_invoice = position(&env, &investor, &invoice)
            .checked_add(new_amount)
            .ok_or(Error::ArithmeticOverflow)?;
        Ok(projected_invoice
            .checked_mul(MAX_BPS as i128)
            .ok_or(Error::ArithmeticOverflow)?
            <= projected_total
                .checked_mul(limit(&env, &investor) as i128)
                .ok_or(Error::ArithmeticOverflow)?)
    }
    pub fn invest(
        env: Env,
        investor: Address,
        invoice: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if !Self::check_concentration(env.clone(), investor.clone(), invoice.clone(), amount)? {
            return Err(Error::ConcentrationLimitExceeded);
        }
        let next_position = position(&env, &investor, &invoice)
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        let next_total = total(&env, &investor)
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        env.storage().persistent().set(
            &DataKey::Position(investor.clone(), invoice.clone()),
            &next_position,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Total(investor.clone()), &next_total);
        env.events()
            .publish((symbol_short!("invest"), investor, invoice), amount);
        Ok(())
    }
    pub fn get_concentration(
        env: Env,
        investor: Address,
        invoice: BytesN<32>,
    ) -> Result<u32, Error> {
        let current_total = total(&env, &investor);
        if current_total == 0 {
            return Ok(0);
        }
        Ok((position(&env, &investor, &invoice)
            .checked_mul(MAX_BPS as i128)
            .ok_or(Error::ArithmeticOverflow)?
            / current_total) as u32)
    }
    pub fn get_position(env: Env, investor: Address, invoice: BytesN<32>) -> i128 {
        position(&env, &investor, &invoice)
    }
    pub fn get_total(env: Env, investor: Address) -> i128 {
        total(&env, &investor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    #[test]
    fn enforces_default_limit() {
        let env = Env::default();
        let contract = env.register(InvestmentLimit, ());
        let client = InvestmentLimitClient::new(&env, &contract);
        let investor = Address::generate(&env);
        let invoice_a = BytesN::from_array(&env, &[1; 32]);
        let invoice_b = BytesN::from_array(&env, &[2; 32]);
        client.invest(&investor, &invoice_a, &100);
        client.invest(&investor, &invoice_b, &400);
        assert_eq!(client.get_concentration(&investor, &invoice_a), 2_000);
        assert!(!client
            .check_concentration(&investor, &invoice_b, &1)
            .unwrap());
    }
}
