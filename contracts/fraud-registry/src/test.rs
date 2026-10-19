#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::types::{FraudReport, ReportOutcome, ReportStatus};

/// Parse an XDR ContractEvent into (contract_addr, topics, data).
fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(
                hash.clone(),
            )),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

/// Check whether an event with the given name was emitted.
fn has_event(env: &Env, name: &str) -> bool {
    let all = env.events().all();
    let target = Symbol::new(env, name);
    for event in all.events().iter() {
        let (_, topics, _) = parse_event(env, event);
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(env, &first) {
                if sym == target {
                    return true;
                }
            }
        }
    }
    false
}

fn test_hash(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

struct Setup {
    env: Env,
    client: FraudRegistryClient<'static>,
    admin: Address,
    investor_a: Address,
    investor_b: Address,
    invoice: BytesN<32>,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, FraudRegistry);
        let client = FraudRegistryClient::new(&env, &contract_id);
        client.initialize(&admin);

        let investor_a = Address::generate(&env);
        let investor_b = Address::generate(&env);
        client.add_investor(&investor_a);
        client.add_investor(&investor_b);

        let invoice = test_hash(&env, "invoice-001");
        Self {
            env,
            client,
            admin,
            investor_a,
            investor_b,
            invoice,
        }
    }
}

#[test]
fn test_submit_report_happy_path() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "evidence-001");

    let report_id = s.client.submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    assert_eq!(report_id, 1);

    let report: FraudReport = s.client.get_report(&report_id);
    assert_eq!(report.invoice, s.invoice);
    assert_eq!(report.reason, reason);
    assert_eq!(report.detail, detail);
    assert_eq!(report.status, ReportStatus::Active);
    assert_eq!(report.resolved, 0);
    assert!(report.resolver.is_none());
    assert_eq!(report.notes, String::from_str(&s.env, ""));
}

#[test]
fn test_submit_report_requires_whitelist() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FraudRegistry);
    let client = FraudRegistryClient::new(&env, &contract_id);
    client.initialize(&admin);

    let random = Address::generate(&env);
    let reason = Symbol::new(&env, "fake_inv");
    let detail = test_hash(&env, "evidence");

    // Not whitelisted — should fail.
    let r = client.try_submit_report(&random, &test_hash(&env, "inv"), &reason, &detail);
    assert!(r.is_err());
}

#[test]
fn test_duplicate_active_report_rejected() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "evidence-001");

    s.client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);

    // Same investor, same invoice — should be rejected.
    let r = s
        .client
        .try_submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    assert!(r.is_err());
}

#[test]
fn test_different_investors_can_report_same_invoice() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "evidence");

    let id1 = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    let id2 = s
        .client
        .submit_report(&s.investor_b, &s.invoice, &reason, &detail);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn test_same_investor_different_invoices() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let inv1 = test_hash(&s.env, "inv1");
    let inv2 = test_hash(&s.env, "inv2");

    s.client
        .submit_report(&s.investor_a, &inv1, &reason, &detail);
    s.client
        .submit_report(&s.investor_a, &inv2, &reason, &detail);
}

#[test]
fn test_resolve_report_confirmed() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let report_id = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    s.env.ledger().set_timestamp(1000);
    s.client.resolve_report(&report_id, &ReportOutcome::Confirmed);

    let report: FraudReport = s.client.get_report(&report_id);
    assert_eq!(report.status, ReportStatus::Confirmed);
    assert!(report.resolved > 0);
    assert_eq!(report.resolver, Some(s.admin.clone()));
}

#[test]
fn test_resolve_report_dismissed() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let report_id = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    s.client.resolve_report(&report_id, &ReportOutcome::Dismissed);

    let report: FraudReport = s.client.get_report(&report_id);
    assert_eq!(report.status, ReportStatus::Dismissed);
}

#[test]
fn test_double_resolve_rejected() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let report_id = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);

    s.client.resolve_report(&report_id, &ReportOutcome::Confirmed);

    // Double resolve fails.
    let r = s
        .client
        .try_resolve_report(&report_id, &ReportOutcome::Dismissed);
    assert!(r.is_err());
}

#[test]
fn test_resolve_nonexistent_report() {
    let s = Setup::new();
    let r = s.client.try_resolve_report(&999, &ReportOutcome::Confirmed);
    assert!(r.is_err());
}

#[test]
fn test_get_reports_by_invoice() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let id1 = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    let id2 = s
        .client
        .submit_report(&s.investor_b, &s.invoice, &reason, &detail);

    let reports: Vec<FraudReport> = s.client.get_reports(&s.invoice);
    assert_eq!(reports.len(), 2);
    assert_eq!(reports.get(0).unwrap().report_id, id1);
    assert_eq!(reports.get(1).unwrap().report_id, id2);
}

#[test]
fn test_get_reports_correct_statuses() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let id1 = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    let id2 = s
        .client
        .submit_report(&s.investor_b, &s.invoice, &reason, &detail);

    s.client.resolve_report(&id1, &ReportOutcome::Confirmed);
    s.client.resolve_report(&id2, &ReportOutcome::Dismissed);

    let reports: Vec<FraudReport> = s.client.get_reports(&s.invoice);
    assert_eq!(reports.get(0).unwrap().status, ReportStatus::Confirmed);
    assert_eq!(reports.get(1).unwrap().status, ReportStatus::Dismissed);
}

#[test]
fn test_fraud_report_submitted_event() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    s.client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);

    assert!(has_event(&s.env, "ReportSubmitted"));
}

#[test]
fn test_fraud_report_resolved_event() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let report_id = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    s.client.resolve_report(&report_id, &ReportOutcome::Confirmed);

    assert!(has_event(&s.env, "ReportResolved"));
}

#[test]
fn test_resolved_allows_new_report_from_same_wallet() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    let id1 = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    s.client.resolve_report(&id1, &ReportOutcome::Confirmed);

    // After resolution, the same investor can file a new report.
    let id2 = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    assert_eq!(id2, 2);
}

#[test]
fn test_add_remove_investor() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FraudRegistry);
    let client = FraudRegistryClient::new(&env, &contract_id);
    client.initialize(&admin);

    let investor = Address::generate(&env);
    client.add_investor(&investor);
    assert!(client.is_investor_whitelisted(&investor));

    client.remove_investor(&investor);
    assert!(!client.is_investor_whitelisted(&investor));

    // Removed investor cannot submit.
    let r = client.try_submit_report(
        &investor,
        &test_hash(&env, "inv"),
        &Symbol::new(&env, "x"),
        &test_hash(&env, "d"),
    );
    assert!(r.is_err());
}

#[test]
fn test_double_add_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FraudRegistry);
    let client = FraudRegistryClient::new(&env, &contract_id);
    client.initialize(&admin);

    let investor = Address::generate(&env);
    client.add_investor(&investor);
    let r = client.try_add_investor(&investor);
    assert!(r.is_err());
}

#[test]
fn test_admin_cannot_submit_report() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    // Admin is not whitelisted and also explicitly blocked.
    let r = s
        .client
        .try_submit_report(&s.admin, &s.invoice, &reason, &detail);
    assert!(r.is_err());
}

#[test]
fn test_duplicate_report_rejected_with_already_reported() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");

    s.client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    let r = s
        .client
        .try_submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    assert_eq!(r, Err(Ok(Error::AlreadyReported)));
}

#[test]
fn test_review_report_records_notes_and_requires_admin() {
    let s = Setup::new();
    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");
    let notes = String::from_str(&s.env, "checked against ledger");

    let report_id = s
        .client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    s.client
        .review_report(&report_id, &ReportOutcome::Dismissed, &notes);

    let report = s.client.get_report(&report_id);
    assert_eq!(report.status, ReportStatus::Dismissed);
    assert_eq!(report.notes, notes);
    assert_eq!(report.resolver, Some(s.admin.clone()));
    assert!(has_event(&s.env, "ReportResolved"));

    // Already resolved reports cannot be reviewed again.
    let again = s
        .client
        .try_review_report(&report_id, &ReportOutcome::Confirmed, &notes);
    assert_eq!(again, Err(Ok(Error::AlreadyResolved)));
}

#[contract]
struct MockInvoiceRegistry;

#[contractimpl]
impl MockInvoiceRegistry {
    pub fn mark_under_review(env: Env, caller: Address, invoice_id: BytesN<32>) {
        caller.require_auth();
        env.storage().instance().set(&invoice_id, &true);
    }

    pub fn is_under_review(env: Env, invoice_id: BytesN<32>) -> bool {
        env.storage().instance().get(&invoice_id).unwrap_or(false)
    }
}

#[test]
fn test_first_report_flags_invoice_under_review() {
    let s = Setup::new();
    let registry_id = s.env.register_contract(None, MockInvoiceRegistry);
    let registry = MockInvoiceRegistryClient::new(&s.env, &registry_id);
    s.client.set_invoice_registry(&registry_id);
    assert_eq!(s.client.get_invoice_registry(), registry_id);

    let reason = Symbol::new(&s.env, "fake_inv");
    let detail = test_hash(&s.env, "ev");
    assert!(!registry.is_under_review(&s.invoice));

    s.client
        .submit_report(&s.investor_a, &s.invoice, &reason, &detail);
    assert!(registry.is_under_review(&s.invoice));

    // Other invoices stay unflagged.
    assert!(!registry.is_under_review(&test_hash(&s.env, "invoice-002")));
}
