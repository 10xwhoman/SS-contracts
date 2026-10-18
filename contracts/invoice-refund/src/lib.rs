//! Invoice Refund contract for StellarSettle (issue #525).
//!
//! Holds investor USDC positions for invoices that must reach a funding
//! threshold before their expiry timestamp. If an invoice expires below the
//! threshold, `trigger_refund` returns every locked position to its investor's
//! wallet, pro-rata by contribution, and emits `RefundProcessed` per investor
//! plus a final `InvoiceRefundComplete`.
//!
//! Triggering is permissionless once the invoice has expired (`now >=
//! expiry_timestamp`) so an admin, keeper, or cron job can all drive it; the
//! expiry check itself always runs, so a refund can never execute early.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

pub use types::{Config, RefundInvoice, RefundSummary};

use errors::Error;

/// Upper bound on unique investors per invoice. Bounds the refund sweep cost
/// so `trigger_refund` always fits within a single transaction.
const MAX_INVESTORS: u32 = 100;

#[contract]
pub struct InvoiceRefund;

#[contractimpl]
impl InvoiceRefund {
    /// Initialize the contract with the admin and the USDC payment token.
    pub fn initialize(env: Env, admin: Address, usdc_token: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_config(
            &env,
            &Config {
                admin: admin.clone(),
                usdc_token: usdc_token.clone(),
            },
        );
        Ok(())
    }

    /// Admin-only: register an invoice that will hold locked USDC until
    /// `expiry_timestamp`. The invoice is refund-eligible on expiry unless its
    /// funding threshold was reached first.
    pub fn register_invoice(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        funding_threshold: i128,
        expiry_timestamp: u64,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if funding_threshold <= 0 {
            return Err(Error::InvalidAmount);
        }
        if expiry_timestamp <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }
        if storage::has_invoice(&env, &invoice_id) {
            return Err(Error::InvoiceAlreadyExists);
        }

        let record = RefundInvoice {
            invoice_id: invoice_id.clone(),
            funding_threshold,
            total_locked: 0,
            expiry_timestamp,
            investors: soroban_sdk::Vec::new(&env),
            refunded: false,
            created_at: env.ledger().timestamp(),
        };
        storage::set_invoice(&env, &invoice_id, &record);
        events::invoice_registered(&env, invoice_id, funding_threshold, expiry_timestamp);
        Ok(())
    }

    /// Lock `amount` of USDC from `investor` into the invoice until expiry or
    /// funding. The investor's position is tracked for a later pro-rata refund.
    pub fn record_investment(
        env: Env,
        invoice_id: Symbol,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let mut record = storage::get_invoice(&env, &invoice_id).ok_or(Error::InvoiceNotFound)?;
        if record.refunded {
            return Err(Error::AlreadyRefunded);
        }
        if env.ledger().timestamp() >= record.expiry_timestamp {
            return Err(Error::InvoiceExpired);
        }

        let new_total = record
            .total_locked
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        if new_total > record.funding_threshold {
            return Err(Error::ExceedsFundingThreshold);
        }

        let current = storage::get_investment(&env, &invoice_id, &investor);
        let new_position = current.checked_add(amount).ok_or(Error::Overflow)?;

        let mut already_recorded = false;
        for existing in record.investors.iter() {
            if existing == investor {
                already_recorded = true;
                break;
            }
        }
        if !already_recorded {
            if record.investors.len() >= MAX_INVESTORS {
                return Err(Error::TooManyInvestors);
            }
            record.investors.push_back(investor.clone());
        }

        // Pull the investor's USDC into this contract; positions stay locked
        // until either the funding threshold is reached (settlement happens
        // elsewhere) or `trigger_refund` returns them.
        token::Client::new(&env, &config.usdc_token).transfer(
            &investor,
            env.current_contract_address(),
            &amount,
        );

        storage::set_investment(&env, &invoice_id, &investor, new_position);
        record.total_locked = new_total;
        storage::set_invoice(&env, &invoice_id, &record);

        events::investment_recorded(&env, invoice_id, &investor, amount, new_total);
        Ok(())
    }

    /// Return every locked position on an expired, under-funded invoice.
    ///
    /// Callable by anyone once `now >= expiry_timestamp` — the admin, a cron
    /// keeper, or a regular user. Rejected while the invoice has not expired,
    /// when it reached its funding threshold, or when it was already refunded.
    pub fn trigger_refund(env: Env, invoice_id: Symbol) -> Result<RefundSummary, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let mut record = storage::get_invoice(&env, &invoice_id).ok_or(Error::InvoiceNotFound)?;

        // Validations: state must be expired and not already refunded.
        if record.refunded {
            return Err(Error::AlreadyRefunded);
        }
        if env.ledger().timestamp() < record.expiry_timestamp {
            return Err(Error::InvoiceNotExpired);
        }
        if record.total_locked >= record.funding_threshold {
            return Err(Error::InvoiceFunded);
        }

        // Mark refunded before any transfer so a re-entrant call cannot
        // double-refund; the whole invocation reverts on any failed transfer.
        record.refunded = true;
        storage::set_invoice(&env, &invoice_id, &record);

        let contract = env.current_contract_address();
        let token = token::Client::new(&env, &config.usdc_token);

        let mut total_refunded: i128 = 0;
        let mut investor_count: u32 = 0;
        for investor in record.investors.iter() {
            let locked = storage::get_investment(&env, &invoice_id, &investor);
            if locked <= 0 {
                continue;
            }
            // Each locked amount is exactly that investor's pro-rata share of
            // the pooled balance, so no rounding residual can be created.
            token.transfer(&contract, &investor, &locked);
            storage::set_investment(&env, &invoice_id, &investor, 0);

            total_refunded = total_refunded.checked_add(locked).ok_or(Error::Overflow)?;
            investor_count += 1;
            events::refund_processed(&env, invoice_id.clone(), &investor, locked);
        }

        let refunded_at = env.ledger().timestamp();
        events::invoice_refund_complete(&env, invoice_id.clone(), total_refunded, investor_count);

        Ok(RefundSummary {
            invoice_id,
            total_refunded,
            investor_count,
            refunded_at,
        })
    }

    /// View: return the global config.
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return the invoice record.
    pub fn get_invoice(env: Env, invoice_id: Symbol) -> Result<RefundInvoice, Error> {
        storage::get_invoice(&env, &invoice_id).ok_or(Error::InvoiceNotFound)
    }

    /// View: return an investor's locked amount (0 when no position exists).
    pub fn get_investment(env: Env, invoice_id: Symbol, investor: Address) -> i128 {
        storage::get_investment(&env, &invoice_id, &investor)
    }

    /// View: whether the invoice's expiry timestamp has passed.
    pub fn is_expired(env: Env, invoice_id: Symbol) -> Result<bool, Error> {
        let record = storage::get_invoice(&env, &invoice_id).ok_or(Error::InvoiceNotFound)?;
        Ok(env.ledger().timestamp() >= record.expiry_timestamp)
    }

    /// View: whether `trigger_refund` would succeed right now.
    pub fn is_refundable(env: Env, invoice_id: Symbol) -> bool {
        let Some(record) = storage::get_invoice(&env, &invoice_id) else {
            return false;
        };
        !record.refunded
            && env.ledger().timestamp() >= record.expiry_timestamp
            && record.total_locked < record.funding_threshold
    }
}

#[cfg(test)]
mod test;
