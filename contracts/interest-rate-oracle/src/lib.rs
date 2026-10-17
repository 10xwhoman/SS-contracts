#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
};

const MAX_RATE_BPS: u32 = 100_000;

#[derive(Clone)]
#[contracttype]
pub struct Rate {
    pub rate_bps: u32,
    pub updated_at: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Rate(BytesN<32>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidRate = 3,
}

#[contract]
pub struct InterestRateOracle;

#[contractimpl]
impl InterestRateOracle {
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
        Ok(())
    }
    pub fn set_rate(env: Env, invoice_id: BytesN<32>, rate_bps: u32) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if rate_bps > MAX_RATE_BPS {
            return Err(Error::InvalidRate);
        }
        let rate = Rate {
            rate_bps,
            updated_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Rate(invoice_id.clone()), &rate);
        env.events()
            .publish((symbol_short!("rate"), invoice_id), rate_bps);
        Ok(())
    }
    pub fn get_rate(env: Env, invoice_id: BytesN<32>) -> Result<Rate, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Rate(invoice_id))
            .ok_or(Error::NotInitialized)
    }
    pub fn get_rate_bps(env: Env, invoice_id: BytesN<32>) -> Result<u32, Error> {
        Ok(Self::get_rate(env, invoice_id)?.rate_bps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    #[test]
    fn admin_updates_invoice_rate() {
        let env = Env::default();
        let contract = env.register(InterestRateOracle, ());
        let client = InterestRateOracleClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let invoice = BytesN::from_array(&env, &[1; 32]);
        client.initialize(&admin);
        client.set_rate(&invoice, &1_250);
        assert_eq!(client.get_rate_bps(&invoice), 1_250);
    }
}
