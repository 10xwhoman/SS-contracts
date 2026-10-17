#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};

const HISTORY_LIMIT: u32 = 32;
const NINETY_DAYS: u64 = 90 * 24 * 60 * 60;
const YEAR: u64 = 365 * 24 * 60 * 60;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Terms(BytesN<32>),
    History(BytesN<32>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InvoiceTerms {
    pub face_value: i128,
    pub maturity: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PricePoint {
    pub ask_price: i128,
    pub quantity: i128,
    pub recorded_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidPrice = 3,
    InvalidTerms = 4,
    NoTerms = 5,
    ArithmeticOverflow = 6,
}

#[contract]
pub struct SecondaryMarketOracle;

#[contractimpl]
impl SecondaryMarketOracle {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn set_invoice_terms(
        env: Env,
        invoice_id: BytesN<32>,
        face_value: i128,
        maturity: u64,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if face_value <= 0 || maturity <= env.ledger().timestamp() {
            return Err(Error::InvalidTerms);
        }
        env.storage().persistent().set(
            &DataKey::Terms(invoice_id),
            &InvoiceTerms {
                face_value,
                maturity,
            },
        );
        Ok(())
    }

    pub fn record_listing_price(
        env: Env,
        invoice_id: BytesN<32>,
        ask_price: i128,
        quantity: i128,
    ) -> Result<(), Error> {
        if ask_price <= 0 || quantity <= 0 {
            return Err(Error::InvalidPrice);
        }
        let history_key = DataKey::History(invoice_id.clone());
        let now = env.ledger().timestamp();
        let mut history: Vec<PricePoint> = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(Vec::new(&env));
        let mut retained = Vec::new(&env);
        for point in history.iter() {
            if now.saturating_sub(point.recorded_at) <= NINETY_DAYS {
                retained.push_back(point);
            }
        }
        retained.push_back(PricePoint {
            ask_price,
            quantity,
            recorded_at: now,
        });
        while retained.len() > HISTORY_LIMIT {
            retained.remove(0);
        }
        env.storage().persistent().set(&history_key, &retained);
        env.events().publish(
            (Symbol::new(&env, "PriceRecorded"), invoice_id),
            (ask_price, quantity, now),
        );
        Ok(())
    }

    pub fn get_price_history(env: Env, invoice_id: BytesN<32>) -> Vec<PricePoint> {
        env.storage()
            .persistent()
            .get(&DataKey::History(invoice_id))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_implied_yield(
        env: Env,
        invoice_id: BytesN<32>,
        ask_price: i128,
    ) -> Result<u32, Error> {
        if ask_price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let terms: InvoiceTerms = env
            .storage()
            .persistent()
            .get(&DataKey::Terms(invoice_id))
            .ok_or(Error::NoTerms)?;
        let remaining = terms.maturity.saturating_sub(env.ledger().timestamp());
        if remaining == 0 {
            return Ok(0);
        }
        let gain = terms
            .face_value
            .checked_sub(ask_price)
            .ok_or(Error::ArithmeticOverflow)?;
        if gain <= 0 {
            return Ok(0);
        }
        let numerator = gain
            .checked_mul(YEAR as i128)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_mul(10_000)
            .ok_or(Error::ArithmeticOverflow)?;
        let denominator = ask_price
            .checked_mul(remaining as i128)
            .ok_or(Error::ArithmeticOverflow)?;
        Ok((numerator / denominator).min(u32::MAX as i128) as u32)
    }
}
