//! Invoice Maturity contract for StellarSettle (issue #536).
//!
//! Tracks each invoice's maturity timestamp and exposes `trigger_maturity`,
//! which becomes callable by anyone (admin, keeper, cron) once the maturity
//! date is reached. Triggering marks the invoice's settlement as initiated and,
//! when a settlement contract is configured, hands off to it via
//! `on_maturity(invoice_id)`.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol};

pub use types::{Config, MaturityInfo, MaturityRecord};

use errors::Error;

/// Seconds in a day, used to derive `days_remaining`.
const SECS_PER_DAY: u64 = 86_400;

#[contract]
pub struct InvoiceMaturity;

#[contractimpl]
impl InvoiceMaturity {
    /// Initialize the contract with the admin. The settlement contract hook is
    /// configured separately via `set_settlement_contract`.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_config(
            &env,
            &Config {
                admin,
                settlement_contract: None,
            },
        );
        Ok(())
    }

    /// Admin-only: set (or clear) the downstream settlement contract invoked
    /// when a maturity is triggered.
    pub fn set_settlement_contract(
        env: Env,
        admin: Address,
        settlement_contract: Option<Address>,
    ) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.settlement_contract = settlement_contract;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: record the maturity timestamp for an invoice, at creation.
    ///
    /// The maturity is set exactly once and must be strictly in the future;
    /// attempting to overwrite an existing maturity is rejected.
    pub fn set_maturity(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        maturity_timestamp: u64,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if maturity_timestamp <= env.ledger().timestamp() {
            return Err(Error::InvalidMaturity);
        }
        if storage::get_maturity(&env, &invoice_id).is_some() {
            return Err(Error::MaturityAlreadySet);
        }

        storage::set_maturity(
            &env,
            &invoice_id,
            &MaturityRecord {
                maturity_timestamp,
                set_at: env.ledger().timestamp(),
                settlement_triggered: false,
                triggered_at: 0,
            },
        );
        events::maturity_set(&env, invoice_id, maturity_timestamp);
        Ok(())
    }

    /// View: whether the invoice has reached its maturity timestamp.
    /// Returns `false` for invoices with no recorded maturity.
    pub fn is_mature(env: Env, invoice_id: Symbol) -> bool {
        match storage::get_maturity(&env, &invoice_id) {
            Some(record) => env.ledger().timestamp() >= record.maturity_timestamp,
            None => false,
        }
    }

    /// Trigger settlement processing for a mature invoice.
    ///
    /// Callable by the admin or any cron/keeper once `now >= maturity_timestamp`;
    /// strictly rejected before maturity. Each invoice can only be triggered
    /// once. When a settlement contract is configured, the hand-off call must
    /// succeed, so an invoice is never marked settled-initiated without the
    /// downstream flow actually starting.
    pub fn trigger_maturity(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let mut record = storage::get_maturity(&env, &invoice_id).ok_or(Error::MaturityNotFound)?;

        if env.ledger().timestamp() < record.maturity_timestamp {
            return Err(Error::NotMature);
        }
        if record.settlement_triggered {
            return Err(Error::SettlementAlreadyTriggered);
        }

        // Hand off to the settlement contract first: if the downstream call
        // fails the whole invocation reverts and the invoice stays triggerable.
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if let Some(target) = config.settlement_contract.clone() {
            env.invoke_contract::<()>(
                &target,
                &Symbol::new(&env, "on_maturity"),
                soroban_sdk::vec![&env, invoice_id.clone().into_val(&env)],
            );
        }

        let triggered_at = env.ledger().timestamp();
        record.settlement_triggered = true;
        record.triggered_at = triggered_at;
        storage::set_maturity(&env, &invoice_id, &record);

        events::maturity_reached(&env, invoice_id.clone(), record.maturity_timestamp);
        events::settlement_triggered(&env, invoice_id, triggered_at);
        Ok(())
    }

    /// View: full maturity info for an invoice, including whole `days_remaining`
    /// (0 once matured or when the current timestamp is past maturity).
    pub fn get_maturity_info(env: Env, invoice_id: Symbol) -> Result<MaturityInfo, Error> {
        let record = storage::get_maturity(&env, &invoice_id).ok_or(Error::MaturityNotFound)?;
        let now = env.ledger().timestamp();
        let is_mature = now >= record.maturity_timestamp;
        let days_remaining = if is_mature {
            0
        } else {
            (record.maturity_timestamp - now) / SECS_PER_DAY
        };
        Ok(MaturityInfo {
            maturity_timestamp: record.maturity_timestamp,
            days_remaining,
            is_mature,
            settlement_triggered: record.settlement_triggered,
        })
    }

    /// View: the raw maturity record for an invoice.
    pub fn get_maturity(env: Env, invoice_id: Symbol) -> Result<MaturityRecord, Error> {
        storage::get_maturity(&env, &invoice_id).ok_or(Error::MaturityNotFound)
    }

    /// View: the global config.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
