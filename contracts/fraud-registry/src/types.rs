//! Data types for the fraud report registry contract.

use soroban_sdk::{contracttype, Address, BytesN, String, Symbol};

/// Lifecycle status of a fraud report.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReportStatus {
    /// Submitted and awaiting admin resolution.
    Active = 0,
    /// Admin confirmed the fraud flag.
    Confirmed = 1,
    /// Admin dismissed the fraud flag.
    Dismissed = 2,
}

/// Resolution outcome supplied by the admin.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReportOutcome {
    Confirmed = 0,
    Dismissed = 1,
}

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config (admin).
    Config,
    /// Instance: initialization flag.
    Initialized,
    /// Persistent: whitelist flag by investor address.
    Whitelist(Address),
    /// Persistent: report record by sequential report id.
    Report(u32),
    /// Persistent: list of report ids filed against an invoice.
    ReportsByInv(BytesN<32>),
    /// Persistent: active report id for (invoice, reporter) pair.
    /// Stores the report id when an active report exists.
    ActiveReport(BytesN<32>, Address),
    /// Instance: next report id counter.
    ReportCount,
    /// Instance: invoice registry contract notified of reported invoices.
    InvoiceRegistry,
}

/// Global configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin authorized to resolve reports and manage whitelist.
    pub admin: Address,
}

/// A fraud report filed by a whitelisted investor.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FraudReport {
    /// Sequential report id.
    pub report_id: u32,
    /// Invoice the flag was raised against.
    pub invoice: BytesN<32>,
    /// Investor who submitted the report.
    pub reporter: Address,
    /// Machine-readable reason category (e.g. "fake_inv", "dbl_pay").
    pub reason: Symbol,
    /// Hash of off-chain supporting evidence.
    pub detail: BytesN<32>,
    /// Current status.
    pub status: ReportStatus,
    /// Ledger timestamp when submitted.
    pub submitted: u64,
    /// Ledger timestamp when resolved (0 while active).
    pub resolved: u64,
    /// Admin that resolved the report (unset while active).
    pub resolver: Option<Address>,
    /// Resolution notes supplied by the admin (empty while active).
    pub notes: String,
}
