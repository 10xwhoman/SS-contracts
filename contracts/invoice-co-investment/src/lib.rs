//! Invoice co-investment contract for StellarSettle (issue #614).
//!
//! Enables multiple verified institutional investors to jointly fund large
//! invoices above standard retail caps, with a lead investor coordinating
//! the syndicate.
//!
//! Flow:
//!   1. Lead calls `create_syndicate` — opens a funding window.
//!   2. Co-investors call `join_syndicate` with ≥ min_co_invest.
//!   3. Once `target_amount` is met, lead calls `execute_syndicate`,
//!      which atomically moves funds to the invoice.
//!   4. If the window closes before the target is met, anyone can call
//!      `cancel_syndicate` to return uncommitted funds.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{CoInvestment, Config, StorageKey, Syndicate, SyndicateStatus};

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

fn load_config(env: &Env) -> Result<Config, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .ok_or(Error::NotInitialized)
}

fn load_syndicate(env: &Env, syndicate_id: u32) -> Result<Syndicate, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Syndicate(syndicate_id))
        .ok_or(Error::SyndicateNotFound)
}

fn save_syndicate(env: &Env, syndicate: &Syndicate) {
    env.storage()
        .persistent()
        .set(&StorageKey::Syndicate(syndicate.syndicate_id), syndicate);
}

fn next_id(env: &Env) -> u32 {
    let id: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::NextId)
        .unwrap_or(1u32);
    env.storage()
        .instance()
        .set(&StorageKey::NextId, &(id + 1));
    id
}

#[contract]
pub struct InvoiceCoInvestment;

#[contractimpl]
impl InvoiceCoInvestment {
    /// Initialise the contract.
    ///
    /// * `admin` – privileged admin address.
    /// * `token` – payment token used for all syndicate transfers.
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&StorageKey::Config, &Config { admin, token });
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Syndicate lifecycle
    // -----------------------------------------------------------------------

    /// Lead investor creates a new syndicate for an invoice.
    ///
    /// * `invoice_id`       – identifier of the invoice to be funded.
    /// * `lead_investor`    – address coordinating the syndicate; must auth.
    /// * `min_co_invest`    – minimum per-co-investor commitment.
    /// * `target_amount`    – total amount required to execute the syndicate.
    /// * `funding_deadline` – Unix timestamp after which the window closes.
    pub fn create_syndicate(
        env: Env,
        invoice_id: Symbol,
        lead_investor: Address,
        min_co_invest: i128,
        target_amount: i128,
        funding_deadline: u64,
    ) -> Result<u32, Error> {
        load_config(&env)?;
        lead_investor.require_auth();

        if min_co_invest <= 0 || target_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if funding_deadline <= env.ledger().timestamp() {
            return Err(Error::FundingWindowExpired);
        }

        let syndicate_id = next_id(&env);
        let syndicate = Syndicate {
            syndicate_id,
            invoice_id: invoice_id.clone(),
            lead_investor: lead_investor.clone(),
            min_co_invest,
            target_amount,
            funding_deadline,
            co_investments: Vec::new(&env),
            total_raised: 0,
            status: SyndicateStatus::Open,
        };
        save_syndicate(&env, &syndicate);

        events::syndicate_created(
            &env,
            syndicate_id,
            &invoice_id,
            &lead_investor,
            target_amount,
            funding_deadline,
        );
        Ok(syndicate_id)
    }

    /// A co-investor commits funds to an open syndicate.
    ///
    /// The committed tokens are transferred from the investor into this
    /// contract immediately (held in escrow until execution or cancellation).
    pub fn join_syndicate(
        env: Env,
        syndicate_id: u32,
        investor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();
        let cfg = load_config(&env)?;
        let mut syndicate = load_syndicate(&env, syndicate_id)?;

        if syndicate.status != SyndicateStatus::Open {
            return Err(Error::AlreadyExecuted);
        }
        if env.ledger().timestamp() > syndicate.funding_deadline {
            return Err(Error::FundingWindowExpired);
        }
        if amount < syndicate.min_co_invest {
            return Err(Error::BelowMinimum);
        }

        // Transfer tokens to the contract (held in escrow).
        token::Client::new(&env, &cfg.token).transfer(
            &investor,
            &env.current_contract_address(),
            &amount,
        );

        syndicate.co_investments.push_back(CoInvestment {
            investor: investor.clone(),
            amount,
        });
        syndicate.total_raised = syndicate
            .total_raised
            .checked_add(amount)
            .ok_or(Error::Overflow)?;

        let total = syndicate.total_raised;
        save_syndicate(&env, &syndicate);

        events::syndicate_joined(&env, syndicate_id, &investor, amount, total);
        Ok(())
    }

    /// Lead investor executes the syndicate when the target is met.
    ///
    /// Transfers the pooled funds to the invoice (represented by the admin
    /// address acting on behalf of the invoice escrow) and marks the
    /// syndicate as Executed.
    pub fn execute_syndicate(env: Env, syndicate_id: u32) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        let mut syndicate = load_syndicate(&env, syndicate_id)?;

        syndicate.lead_investor.require_auth();

        if syndicate.status == SyndicateStatus::Executed {
            return Err(Error::AlreadyExecuted);
        }
        if syndicate.status == SyndicateStatus::Cancelled {
            return Err(Error::AlreadyCancelled);
        }
        if syndicate.total_raised < syndicate.target_amount {
            return Err(Error::TargetNotMet);
        }

        // Transfer accumulated funds to the invoice escrow (admin in this
        // simplified model; production would call a target invoice contract).
        token::Client::new(&env, &cfg.token).transfer(
            &env.current_contract_address(),
            &cfg.admin,
            &syndicate.total_raised,
        );

        syndicate.status = SyndicateStatus::Executed;
        let total = syndicate.total_raised;
        let invoice_id = syndicate.invoice_id.clone();
        save_syndicate(&env, &syndicate);

        events::syndicate_executed(&env, syndicate_id, &invoice_id, total);
        Ok(())
    }

    /// Cancel a syndicate whose funding window has expired without meeting
    /// the target, returning all committed funds to their respective investors.
    pub fn cancel_syndicate(env: Env, syndicate_id: u32) -> Result<(), Error> {
        let cfg = load_config(&env)?;
        let mut syndicate = load_syndicate(&env, syndicate_id)?;

        if syndicate.status == SyndicateStatus::Executed {
            return Err(Error::AlreadyExecuted);
        }
        if syndicate.status == SyndicateStatus::Cancelled {
            return Err(Error::AlreadyCancelled);
        }
        if env.ledger().timestamp() <= syndicate.funding_deadline {
            return Err(Error::FundingWindowExpired);
        }

        let tok = token::Client::new(&env, &cfg.token);
        let mut total_returned: i128 = 0;
        for i in 0..syndicate.co_investments.len() {
            let entry = syndicate.co_investments.get_unchecked(i);
            tok.transfer(&env.current_contract_address(), &entry.investor, &entry.amount);
            total_returned = total_returned
                .checked_add(entry.amount)
                .ok_or(Error::Overflow)?;
        }

        syndicate.status = SyndicateStatus::Cancelled;
        save_syndicate(&env, &syndicate);

        events::syndicate_cancelled(&env, syndicate_id, total_returned);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read helpers
    // -----------------------------------------------------------------------

    /// Returns the full syndicate record.
    pub fn get_syndicate(env: Env, syndicate_id: u32) -> Result<Syndicate, Error> {
        load_config(&env)?;
        load_syndicate(&env, syndicate_id)
    }
}
