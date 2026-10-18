//! Multi-Currency Oracle Price Adapter interface for foreign currency denominated invoices (#480).
//!
//! Enables invoice escrow contracts to accept and settle invoices denominated in
//! foreign currencies (e.g. EUR, GBP, NGN, BRL, JPY) by fetching exchange rates
//! from Stellar/Soroban oracle feeds, validating against staleness, and converting
//! to the base settlement token (USDC / stroops).

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Error types for oracle price operations.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleError {
    NotInitialized = 1,
    Unauthorized = 2,
    UnsupportedCurrency = 3,
    PriceStale = 4,
    InvalidPrice = 5,
    MathOverflow = 6,
    ZeroAmount = 7,
}

/// Price data record with price value, decimals, and timestamp.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    /// Normalized price in base currency (e.g. 1 EUR = 1.08 USD, price = 108_000_000 with 8 decimals).
    pub price: i128,
    /// Decimal precision for the price quote.
    pub decimals: u32,
    /// Timestamp of the price update in unix seconds.
    pub timestamp: u64,
}

/// Currency configuration for an external oracle feed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrencyOracleConfig {
    pub currency: Symbol,
    pub oracle: Address,
    pub max_staleness_seconds: u64,
    pub is_active: bool,
}

/// Storage keys for oracle adapter configuration and rates.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleStorageKey {
    /// Instance: admin address.
    Admin,
    /// Instance: default base currency symbol (e.g. "USD" or "USDC").
    BaseCurrency,
    /// Persistent: oracle config per foreign currency symbol.
    CurrencyConfig(Symbol),
    /// Persistent: latest manual or cached price per foreign currency symbol.
    CachedPrice(Symbol),
}

/// Event emitted when an oracle configuration is added or updated.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleConfiguredEvent {
    pub currency: Symbol,
    pub oracle: Address,
    pub max_staleness_seconds: u64,
    pub is_active: bool,
}

/// Event emitted when a price is updated or cached.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OraclePriceUpdatedEvent {
    pub currency: Symbol,
    pub price: i128,
    pub decimals: u32,
    pub timestamp: u64,
}

/// Event emitted when an amount is converted between foreign currency and base settlement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrencyConvertedEvent {
    pub foreign_currency: Symbol,
    pub foreign_amount: i128,
    pub base_amount: i128,
    pub conversion_price: i128,
}

pub struct OraclePriceAdapter;

impl OraclePriceAdapter {
    /// Default maximum acceptable staleness window: 3,600 seconds (1 hour).
    pub const DEFAULT_MAX_STALENESS_SECS: u64 = 3_600;

    /// Sets the admin for the oracle adapter.
    pub fn init_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&OracleStorageKey::Admin, admin);
    }

    /// Retrieves the admin address.
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&OracleStorageKey::Admin)
    }

    /// Sets the base currency symbol (e.g. "USD").
    pub fn set_base_currency(env: &Env, admin: &Address, base_currency: &Symbol) -> Result<(), OracleError> {
        admin.require_auth();
        if let Some(current_admin) = Self::get_admin(env) {
            if current_admin != *admin {
                return Err(OracleError::Unauthorized);
            }
        }
        env.storage().instance().set(&OracleStorageKey::BaseCurrency, base_currency);
        Ok(())
    }

    /// Retrieves the base currency symbol.
    pub fn get_base_currency(env: &Env) -> Symbol {
        env.storage()
            .instance()
            .get(&OracleStorageKey::BaseCurrency)
            .unwrap_or_else(|| Symbol::new(env, "USD"))
    }

    /// Configures or updates the oracle contract address and max staleness for a currency.
    pub fn set_currency_oracle(
        env: &Env,
        admin: &Address,
        currency: Symbol,
        oracle: Address,
        max_staleness_seconds: u64,
    ) -> Result<(), OracleError> {
        admin.require_auth();
        if let Some(current_admin) = Self::get_admin(env) {
            if current_admin != *admin {
                return Err(OracleError::Unauthorized);
            }
        }

        let staleness = if max_staleness_seconds == 0 {
            Self::DEFAULT_MAX_STALENESS_SECS
        } else {
            max_staleness_seconds
        };

        let config = CurrencyOracleConfig {
            currency: currency.clone(),
            oracle: oracle.clone(),
            max_staleness_seconds: staleness,
            is_active: true,
        };

        env.storage().persistent().set(&OracleStorageKey::CurrencyConfig(currency.clone()), &config);

        env.events().publish(
            (Symbol::new(env, "oracle"), Symbol::new(env, "configured")),
            OracleConfiguredEvent {
                currency,
                oracle,
                max_staleness_seconds: staleness,
                is_active: true,
            },
        );

        Ok(())
    }

    /// Sets or updates a manual/cached price for a currency (useful for direct feeds or testing).
    pub fn set_price(
        env: &Env,
        admin: &Address,
        currency: Symbol,
        price: i128,
        decimals: u32,
        timestamp: u64,
    ) -> Result<(), OracleError> {
        admin.require_auth();
        if let Some(current_admin) = Self::get_admin(env) {
            if current_admin != *admin {
                return Err(OracleError::Unauthorized);
            }
        }

        if price <= 0 {
            return Err(OracleError::InvalidPrice);
        }

        let price_data = PriceData {
            price,
            decimals,
            timestamp,
        };

        env.storage().persistent().set(&OracleStorageKey::CachedPrice(currency.clone()), &price_data);

        env.events().publish(
            (Symbol::new(env, "oracle"), Symbol::new(env, "price_updated")),
            OraclePriceUpdatedEvent {
                currency,
                price,
                decimals,
                timestamp,
            },
        );

        Ok(())
    }

    /// Fetches the latest price for a foreign currency. Checks for staleness.
    pub fn get_price(env: &Env, currency: &Symbol) -> Result<PriceData, OracleError> {
        let price_data: PriceData = env
            .storage()
            .persistent()
            .get(&OracleStorageKey::CachedPrice(currency.clone()))
            .ok_or(OracleError::UnsupportedCurrency)?;

        let config: Option<CurrencyOracleConfig> = env
            .storage()
            .persistent()
            .get(&OracleStorageKey::CurrencyConfig(currency.clone()));

        let max_staleness = config
            .map(|c| c.max_staleness_seconds)
            .unwrap_or(Self::DEFAULT_MAX_STALENESS_SECS);

        let current_time = env.ledger().timestamp();
        if current_time > price_data.timestamp {
            let age = current_time - price_data.timestamp;
            if age > max_staleness {
                return Err(OracleError::PriceStale);
            }
        }

        if price_data.price <= 0 {
            return Err(OracleError::InvalidPrice);
        }

        Ok(price_data)
    }

    /// Converts an amount in foreign currency to base settlement token units.
    ///
    /// Formula:
    /// `base_amount = (foreign_amount * price) / 10^decimals`
    pub fn convert_to_base(
        env: &Env,
        foreign_currency: &Symbol,
        foreign_amount: i128,
    ) -> Result<i128, OracleError> {
        if foreign_amount <= 0 {
            return Err(OracleError::ZeroAmount);
        }

        let price_data = Self::get_price(env, foreign_currency)?;

        let scale = 10_i128
            .checked_pow(price_data.decimals)
            .ok_or(OracleError::MathOverflow)?;

        let base_amount = foreign_amount
            .checked_mul(price_data.price)
            .ok_or(OracleError::MathOverflow)?
            .checked_div(scale)
            .ok_or(OracleError::MathOverflow)?;

        env.events().publish(
            (Symbol::new(env, "oracle"), Symbol::new(env, "converted")),
            CurrencyConvertedEvent {
                foreign_currency: foreign_currency.clone(),
                foreign_amount,
                base_amount,
                conversion_price: price_data.price,
            },
        );

        Ok(base_amount)
    }

    /// Converts an amount in base settlement token units to foreign currency units.
    ///
    /// Formula:
    /// `foreign_amount = (base_amount * 10^decimals) / price`
    pub fn convert_from_base(
        env: &Env,
        foreign_currency: &Symbol,
        base_amount: i128,
    ) -> Result<i128, OracleError> {
        if base_amount <= 0 {
            return Err(OracleError::ZeroAmount);
        }

        let price_data = Self::get_price(env, foreign_currency)?;

        let scale = 10_i128
            .checked_pow(price_data.decimals)
            .ok_or(OracleError::MathOverflow)?;

        let foreign_amount = base_amount
            .checked_mul(scale)
            .ok_or(OracleError::MathOverflow)?
            .checked_div(price_data.price)
            .ok_or(OracleError::MathOverflow)?;

        Ok(foreign_amount)
    }
}
