//! Fraud report registry contract for StellarSettle.
//!
//! Whitelisted investors submit fraud flags against invoices. An admin
//! resolves reports (confirmed / dismissed). All submissions and resolutions
//! are recorded on-chain with events for audit trails. The first report filed
//! against an invoice flags it as under review in the invoice registry.

#![no_std]
#![allow(clippy::too_many_arguments, deprecated)]

mod errors;
mod events;
mod storage;
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, IntoVal, String, Symbol, Vec};

use errors::Error;
use types::{Config, FraudReport, ReportOutcome, ReportStatus};

#[contract]
pub struct FraudRegistry;

#[contractimpl]
impl FraudRegistry {
    /// Initialize the contract with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInit);
        }
        admin.require_auth();
        storage::set_config(&env, &Config { admin });
        storage::set_initialized(&env);
        Ok(())
    }

    // ── Whitelist management ──────────────────────────────────────────

    /// Admin-only: add an investor to the submission whitelist.
    pub fn add_investor(env: Env, investor: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        if storage::is_whitelisted(&env, &investor) {
            return Err(Error::AlreadyWhitelisted);
        }
        storage::set_whitelisted(&env, &investor, true);
        Ok(())
    }

    /// Admin-only: remove an investor from the submission whitelist.
    pub fn remove_investor(env: Env, investor: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        if !storage::is_whitelisted(&env, &investor) {
            return Err(Error::NotWhitelistedErr);
        }
        storage::set_whitelisted(&env, &investor, false);
        Ok(())
    }

    /// View: is an investor whitelisted to submit reports.
    pub fn is_investor_whitelisted(env: Env, investor: Address) -> Result<bool, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::is_whitelisted(&env, &investor))
    }

    /// Admin-only: set the invoice registry that is told when an invoice
    /// receives its first fraud report.
    pub fn set_invoice_registry(env: Env, registry: Address) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        storage::set_invoice_registry(&env, &registry);
        Ok(())
    }

    /// View: the configured invoice registry.
    pub fn get_invoice_registry(env: Env) -> Result<Address, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        storage::get_invoice_registry(&env).ok_or(Error::RegistryNotSet)
    }

    // ── Report lifecycle ───────────────────────────────────────────────

    /// Submit a fraud report against an invoice. Restricted to whitelisted
    /// investors. Enforces one active report per wallet per invoice.
    pub fn submit_report(
        env: Env,
        reporter: Address,
        invoice_id: BytesN<32>,
        reason_code: Symbol,
        detail_hash: BytesN<32>,
    ) -> Result<u32, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        reporter.require_auth();

        if !storage::is_whitelisted(&env, &reporter) {
            return Err(Error::NotWhitelisted);
        }
        // Admin is not allowed to submit (separation of concerns); but the
        // spec only restricts to whitelisted investors, so we allow anyone the
        // admin whitelisted. We do block the admin itself from reporting to
        // keep roles separate.
        if reporter == config.admin {
            return Err(Error::Unauthorized);
        }

        // Enforce one active report per wallet per invoice.
        if storage::get_active_report(&env, &invoice_id, &reporter).is_some() {
            return Err(Error::AlreadyReported);
        }

        // First report against this invoice: flag it as under review.
        if storage::get_reports_by_invoice(&env, &invoice_id).is_empty() {
            if let Some(registry) = storage::get_invoice_registry(&env) {
                env.invoke_contract::<()>(
                    &registry,
                    &Symbol::new(&env, "mark_under_review"),
                    (env.current_contract_address(), invoice_id.clone()).into_val(&env),
                );
            }
        }

        let report_id = storage::next_report_id(&env);
        let now = env.ledger().timestamp();
        let report = FraudReport {
            report_id,
            invoice: invoice_id.clone(),
            reporter: reporter.clone(),
            reason: reason_code.clone(),
            detail: detail_hash.clone(),
            status: ReportStatus::Active,
            submitted: now,
            resolved: 0,
            resolver: None,
            notes: String::from_str(&env, ""),
        };
        storage::set_report(&env, &report);
        storage::set_active_report(&env, &invoice_id, &reporter, report_id);
        storage::add_report_to_invoice(&env, &invoice_id, report_id);

        events::fraud_report_submitted(
            &env,
            report_id,
            &invoice_id,
            &reporter,
            &reason_code,
            &detail_hash,
            now,
        );
        Ok(report_id)
    }

    /// Admin-only: resolve a report as confirmed or dismissed, without notes.
    pub fn resolve_report(
        env: Env,
        report_id: u32,
        outcome: ReportOutcome,
    ) -> Result<(), Error> {
        let notes = String::from_str(&env, "");
        Self::review_report(env, report_id, outcome, notes)
    }

    /// Admin-only: review a report, recording the resolution (confirmed or
    /// dismissed) together with the admin's resolution notes.
    pub fn review_report(
        env: Env,
        report_id: u32,
        resolution: ReportOutcome,
        notes: String,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();

        let mut report = storage::get_report(&env, report_id).ok_or(Error::ReportNotFound)?;
        if report.status != ReportStatus::Active {
            return Err(Error::AlreadyResolved);
        }

        let new_status = match resolution {
            ReportOutcome::Confirmed => ReportStatus::Confirmed,
            ReportOutcome::Dismissed => ReportStatus::Dismissed,
        };
        report.status = new_status;
        report.resolved = env.ledger().timestamp();
        report.resolver = Some(config.admin.clone());
        report.notes = notes.clone();
        storage::set_report(&env, &report);

        // Clear the active-report lock so the reporter can file a new one if needed.
        storage::clear_active_report(&env, &report.invoice, &report.reporter);

        events::fraud_report_resolved(
            &env,
            report_id,
            &report.invoice,
            &config.admin,
            new_status as u32,
            &notes,
            report.resolved,
        );
        Ok(())
    }

    // ── Views ──────────────────────────────────────────────────────────

    /// View: returns a single report by id.
    pub fn get_report(env: Env, report_id: u32) -> Result<FraudReport, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        storage::get_report(&env, report_id).ok_or(Error::ReportNotFound)
    }

    /// View: returns all reports filed against an invoice, with their statuses.
    pub fn get_reports(env: Env, invoice_id: BytesN<32>) -> Result<Vec<FraudReport>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        let ids = storage::get_reports_by_invoice(&env, &invoice_id);
        let mut reports = Vec::new(&env);
        for id in ids.iter() {
            if let Some(report) = storage::get_report(&env, id) {
                reports.push_back(report);
            }
        }
        Ok(reports)
    }

    /// View: returns the active report id for (invoice, reporter), if any.
    pub fn get_active_report(
        env: Env,
        invoice_id: BytesN<32>,
        reporter: Address,
    ) -> Result<Option<u32>, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_active_report(&env, &invoice_id, &reporter))
    }
}
