#![no_std]

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol};

const BPS_DENOMINATOR: i128 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidMultiplier = 3,
    InvalidAmount = 4,
    InvoiceNotRecorded = 5,
    PriceCapExceeded = 6,
    Overflow = 7,
}

/// Settlement data a price cap is derived from.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementRecord {
    /// Price per fraction at settlement.
    pub base_price: i128,
    /// Yield rate realised at settlement, in basis points.
    pub yield_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    MultiplierBps,
    Settlement(BytesN<32>),
}

#[contract]
pub struct PriceCap;

#[contractimpl]
impl PriceCap {
    /// `multiplier_bps` of 15000 caps the yield a listing may price in at 1.5x the settled rate.
    pub fn initialize(env: Env, admin: Address, multiplier_bps: u32) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if multiplier_bps == 0 {
            return Err(Error::InvalidMultiplier);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::MultiplierBps, &multiplier_bps);
        Ok(())
    }

    #[allow(deprecated)]
    pub fn set_price_cap_multiplier(env: Env, multiplier_bps: u32) -> Result<(), Error> {
        Self::require_admin(&env)?;
        if multiplier_bps == 0 {
            return Err(Error::InvalidMultiplier);
        }
        let old = Self::multiplier(&env)?;
        env.storage()
            .instance()
            .set(&StorageKey::MultiplierBps, &multiplier_bps);
        let topics = (Symbol::new(&env, "PriceCapUpdated"),);
        env.events().publish(topics, (old, multiplier_bps));
        Ok(())
    }

    /// Records the base price and yield rate of an invoice's last settlement.
    pub fn record_settlement(
        env: Env,
        invoice_id: BytesN<32>,
        base_price: i128,
        yield_bps: u32,
    ) -> Result<(), Error> {
        Self::require_admin(&env)?;
        if base_price <= 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage().persistent().set(
            &StorageKey::Settlement(invoice_id),
            &SettlementRecord {
                base_price,
                yield_bps,
            },
        );
        Ok(())
    }

    /// Maximum price per fraction: `base_price * (1 + yield * multiplier)`.
    pub fn get_price_cap(env: Env, invoice_id: BytesN<32>) -> Result<i128, Error> {
        let multiplier = Self::multiplier(&env)?;
        let record: SettlementRecord = env
            .storage()
            .persistent()
            .get(&StorageKey::Settlement(invoice_id))
            .ok_or(Error::InvoiceNotRecorded)?;
        // Capped yield in bps-of-bps scale: yield_bps * multiplier_bps / 10_000.
        let capped_yield_bps = (record.yield_bps as i128)
            .checked_mul(multiplier as i128)
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;
        let uplift = record
            .base_price
            .checked_mul(capped_yield_bps)
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;
        record.base_price.checked_add(uplift).ok_or(Error::Overflow)
    }

    /// Called by the listing flow before accepting a listing; fails when `price` exceeds the cap.
    pub fn validate_listing_price(
        env: Env,
        invoice_id: BytesN<32>,
        price: i128,
    ) -> Result<(), Error> {
        if price <= 0 {
            return Err(Error::InvalidAmount);
        }
        if price > Self::get_price_cap(env, invoice_id)? {
            return Err(Error::PriceCapExceeded);
        }
        Ok(())
    }

    pub fn get_price_cap_multiplier(env: Env) -> Result<u32, Error> {
        Self::multiplier(&env)
    }
}

impl PriceCap {
    fn multiplier(env: &Env) -> Result<u32, Error> {
        env.storage()
            .instance()
            .get(&StorageKey::MultiplierBps)
            .ok_or(Error::NotInitialized)
    }

    fn require_admin(env: &Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }
}
