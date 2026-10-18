#![allow(deprecated)]
//! Event definitions for the fraud report registry contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};

/// Emit a `FraudReportSubmitted` event when a new report is filed.
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
            Symbol::new(env, "FraudReportSubmitted"),
            report_id,
        ),
        (invoice.clone(), reporter.clone(), reason.clone(), detail.clone(), timestamp),
    );
}

/// Emit a `FraudReportResolved` event when an admin resolves a report.
pub fn fraud_report_resolved(
    env: &Env,
    report_id: u32,
    invoice: &BytesN<32>,
    resolver: &Address,
    outcome: u32,
    timestamp: u64,
) {
    env.events().publish(
        (
            Symbol::new(env, "FraudReportResolved"),
            report_id,
        ),
        (invoice.clone(), resolver.clone(), outcome, timestamp),
    );
}
