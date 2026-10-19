//! Secondary Market Fee contract for StellarSettle.
//!
//! Collects the platform fee on secondary market fraction sales. On every
//! sale, the fee is computed from the configured basis-point rate, the net
//! amount (`sale_amount - fee`) is returned to the seller, and the fee is
//! routed directly to the treasury contract.
//!
//! The authorized secondary market contract either deposits `sale_amount` into
//! this contract before calling `collect_secondary_fee`, or holds the funds
//! itself — the contract first spends its own balance if it covers the sale
//! and otherwise transfers from the calling market contract. Either way the
//! seller receives the net amount and the treasury receives the fee.
//!
//! It also charges a listing fee when a seller lists fractions for sale: a flat
//! rate per fraction listed, configured by the admin per invoice sector and
//! routed to the treasury (`get_listing_fee` / `collect_listing_fee`).

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{FeeReceipt, RegistryAssignment, Sector};

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, IntoVal, String, Symbol};

use errors::Error;

/// Maximum allowed platform fee: 10,000 bps = 100%.
const MAX_FEE_BPS: u32 = 10_000;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

/// Compute the seller/treasury split for `sale_amount` at `fee_bps`.
/// The fee is floored to whole units and the seller absorbs the rounding dust,
/// so `net_amount + fee_amount == sale_amount` exactly.
fn compute_fee(
    sale_amount: i128,
    fee_bps: u32,
) -> Result<FeeReceipt, Error> {
    if sale_amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    let fee_amount = sale_amount
        .checked_mul(fee_bps as i128)
        .ok_or(Error::Overflow)?
        .checked_div(MAX_FEE_BPS as i128)
        .ok_or(Error::Overflow)?;
    let net_amount = sale_amount
        .checked_sub(fee_amount)
        .ok_or(Error::Overflow)?;
    Ok(FeeReceipt {
        sale_amount,
        net_amount,
        fee_amount,
    })
}

/// Resolve an invoice's sector through the configured invoice registry.
fn invoice_sector(env: &Env, invoice_id: &BytesN<32>) -> Result<Sector, Error> {
    let registry = storage::get_invoice_registry(env).ok_or(Error::RegistryNotSet)?;
    match env.try_invoke_contract::<RegistryAssignment, soroban_sdk::Error>(
        &registry,
        &Symbol::new(env, "get_category"),
        (invoice_id.clone(),).into_val(env),
    ) {
        Ok(Ok(assignment)) => Ok(assignment.category),
        _ => Err(Error::InvoiceNotRegistered),
    }
}

/// Total listing fee for `amount` fractions of an invoice.
fn listing_fee(env: &Env, invoice_id: &BytesN<32>, amount: i128) -> Result<i128, Error> {
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    let sector = invoice_sector(env, invoice_id)?;
    storage::get_listing_fee(env, sector)
        .checked_mul(amount)
        .ok_or(Error::Overflow)
}

#[contract]
pub struct SecondaryFee;

#[contractimpl]
impl SecondaryFee {
    /// Initialize the contract with the admin, treasury destination, and the
    /// initial platform fee rate in basis points (e.g. 250 = 2.5%).
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
    ) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        ensure_non_zero_address(&env, &treasury)?;
        if treasury == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFeeBps);
        }
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        storage::set_treasury(&env, &treasury);
        storage::set_fee_bps(&env, fee_bps);
        events::initialized(&env, &admin, &treasury, fee_bps);
        Ok(())
    }

    /// Admin-only: update the platform fee rate in basis points.
    pub fn set_fee_bps(env: Env, admin: Address, new_fee_bps: u32) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        if new_fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFeeBps);
        }
        admin.require_auth();
        let old_bps = storage::get_fee_bps(&env);
        storage::set_fee_bps(&env, new_fee_bps);
        events::fee_bps_updated(&env, old_bps, new_fee_bps);
        Ok(())
    }

    /// Admin-only: update the treasury contract that receives fees.
    pub fn set_treasury(env: Env, admin: Address, new_treasury: Address) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &new_treasury)?;
        if new_treasury == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        admin.require_auth();
        let old_treasury = storage::get_treasury(&env);
        storage::set_treasury(&env, &new_treasury);
        events::treasury_updated(&env, old_treasury, &new_treasury);
        Ok(())
    }

    /// Admin-only: bind the secondary market contract allowed to collect fees.
    pub fn set_secondary_market(
        env: Env,
        admin: Address,
        new_market: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &new_market)?;
        if new_market == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        admin.require_auth();
        let old_market = storage::get_secondary_market(&env);
        storage::set_secondary_market(&env, &new_market);
        events::secondary_market_updated(&env, old_market, &new_market);
        Ok(())
    }

    /// Collect the platform fee on a secondary market fraction sale.
    ///
    /// Must be called by the secondary market contract bound via
    /// `set_secondary_market`. `sale_amount` must already be held by this
    /// contract or by the calling market contract. Emits
    /// `SecondaryFeeCollected` with the listing id and fee amount.
    ///
    /// Returns the computed `FeeReceipt` (sale, net, fee).
    pub fn collect_secondary_fee(
        env: Env,
        caller: Address,
        listing_id: Symbol,
        sale_amount: i128,
        seller: Address,
        token: Address,
    ) -> Result<FeeReceipt, Error> {
        // Ensure the contract is initialized before checking the market binding.
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        let market = storage::get_secondary_market(&env).ok_or(Error::SecondaryMarketNotSet)?;
        if caller != market {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        ensure_non_zero_address(&env, &seller)?;
        ensure_non_zero_address(&env, &token)?;

        let fee_bps = storage::get_fee_bps(&env);
        let receipt = compute_fee(sale_amount, fee_bps)?;

        let treasury = storage::get_treasury(&env).ok_or(Error::NotInit)?;
        let contract = env.current_contract_address();
        let token_client = token::Client::new(&env, &token);

        if token_client.balance(&contract) >= sale_amount {
            // Funds were pre-deposited into this contract.
            if receipt.net_amount > 0 {
                token_client.transfer(&contract, &seller, &receipt.net_amount);
            }
            if receipt.fee_amount > 0 {
                token_client.transfer(&contract, &treasury, &receipt.fee_amount);
            }
        } else {
            // The market contract holds the sale proceeds: split them straight
            // from the market to the seller and treasury.
            if token_client.balance(&caller) < sale_amount {
                return Err(Error::InsufficientBalance);
            }
            if receipt.net_amount > 0 {
                token_client.transfer(&caller, &seller, &receipt.net_amount);
            }
            if receipt.fee_amount > 0 {
                token_client.transfer(&caller, &treasury, &receipt.fee_amount);
            }
        }

        events::secondary_fee_collected(&env, &listing_id, &seller, &token, &receipt);
        Ok(receipt)
    }

    /// Admin-only: bind the invoice registry used to resolve an invoice's sector.
    pub fn set_invoice_registry(
        env: Env,
        admin: Address,
        new_registry: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &new_registry)?;
        admin.require_auth();
        let old_registry = storage::get_invoice_registry(&env);
        storage::set_invoice_registry(&env, &new_registry);
        events::invoice_registry_updated(&env, old_registry, &new_registry);
        Ok(())
    }

    /// Admin-only: set the flat listing fee charged per fraction listed for a
    /// sector. A fee of `0` disables the listing fee for that sector.
    pub fn set_listing_fee(
        env: Env,
        admin: Address,
        sector: Sector,
        fee_per_fraction: i128,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        if fee_per_fraction < 0 {
            return Err(Error::InvalidAmount);
        }
        admin.require_auth();
        let old_fee = storage::get_listing_fee(&env, sector);
        storage::set_listing_fee(&env, sector, fee_per_fraction);
        events::listing_fee_updated(&env, sector, old_fee, fee_per_fraction);
        Ok(())
    }

    /// View: total listing fee for listing `amount` fractions of an invoice,
    /// at the flat per-fraction rate of the invoice's sector.
    pub fn get_listing_fee(env: Env, invoice_id: BytesN<32>, amount: i128) -> Result<i128, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        listing_fee(&env, &invoice_id, amount)
    }

    /// Charge the listing fee for creating a listing of `amount` fractions.
    ///
    /// Must be called by the secondary market contract bound via
    /// `set_secondary_market` from inside its listing creation, so a seller who
    /// cannot cover the fee gets the whole listing rejected. The fee is moved
    /// from `seller` to the treasury and `ListingFeeCollected` is emitted.
    ///
    /// Returns the fee collected.
    pub fn collect_listing_fee(
        env: Env,
        caller: Address,
        seller: Address,
        invoice_id: BytesN<32>,
        amount: i128,
        token: Address,
    ) -> Result<i128, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        let market = storage::get_secondary_market(&env).ok_or(Error::SecondaryMarketNotSet)?;
        if caller != market {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        ensure_non_zero_address(&env, &seller)?;
        ensure_non_zero_address(&env, &token)?;

        let fee = listing_fee(&env, &invoice_id, amount)?;
        if fee > 0 {
            let treasury = storage::get_treasury(&env).ok_or(Error::NotInit)?;
            let token_client = token::Client::new(&env, &token);
            if token_client.balance(&seller) < fee {
                return Err(Error::InsufficientBalance);
            }
            token_client.transfer(&seller, &treasury, &fee);
        }

        events::listing_fee_collected(&env, &invoice_id, &seller, &token, amount, fee);
        Ok(fee)
    }

    /// View: preview the seller/treasury split for a hypothetical sale.
    pub fn preview_fee(env: Env, sale_amount: i128) -> Result<FeeReceipt, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        compute_fee(sale_amount, storage::get_fee_bps(&env))
    }

    /// View: current platform fee rate in basis points.
    pub fn get_fee_bps(env: Env) -> Result<u32, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_fee_bps(&env))
    }

    /// View: treasury contract that receives fees.
    pub fn get_treasury(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        storage::get_treasury(&env).ok_or(Error::NotInit)
    }

    /// View: authorized secondary market contract, if bound.
    pub fn get_secondary_market(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        storage::get_secondary_market(&env).ok_or(Error::SecondaryMarketNotSet)
    }

    /// View: contract admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
