#![allow(deprecated)]
//! Event definitions for the fraud report registry contract.

use soroban_sdk::{Address, BytesN, Env, String, Symbol};

/// Emit a `ReportSubmitted` event when a new report is filed.
pub fn fraud_report_submitted(
    env: &Env,
    report_id: u32,
    invoice: &BytesN<32>,
    reporter: &Address,
    reason: &Symbol,
    detail: &BytesN<32>,
    timestamp: u64,
) {
    env.events().publish(
        (
            Symbol::new(env, "ReportSubmitted"),
            report_id,
        ),
        (invoice.clone(), reporter.clone(), reason.clone(), detail.clone(), timestamp),
    );
}

/// Emit a `ReportResolved` event when an admin resolves a report.
pub fn fraud_report_resolved(
    env: &Env,
    report_id: u32,
    invoice: &BytesN<32>,
    resolver: &Address,
    outcome: u32,
    notes: &String,
    timestamp: u64,
) {
    env.events().publish(
        (
            Symbol::new(env, "ReportResolved"),
            report_id,
        ),
        (invoice.clone(), resolver.clone(), outcome, notes.clone(), timestamp),
    );
}
