//! Data types for the invoice maturity contract.

use soroban_sdk::{contracttype, Address, Symbol};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config (admin + optional settlement target).
    Config,
    /// Persistent: maturity record by invoice id.
    Maturity(Symbol),
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin authorized to set maturities and configuration.
    pub admin: Address,
    /// Optional settlement contract invoked with `on_maturity(invoice_id)`
    /// when a maturity is triggered, to kick off downstream settlement
    /// processing. When unset, triggering only records the state transition.
    pub settlement_contract: Option<Address>,
}

/// Maturity state for a single invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaturityRecord {
    /// Ledger timestamp the invoice matures at.
    pub maturity_timestamp: u64,
    /// Ledger timestamp the maturity was configured at.
    pub set_at: u64,
    /// Set once `trigger_maturity` has run for this invoice.
    pub settlement_triggered: bool,
    /// Ledger timestamp `trigger_maturity` ran at (0 before triggering).
    pub triggered_at: u64,
}

/// Read-only view returned by `get_maturity_info`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaturityInfo {
    /// Ledger timestamp the invoice matures at.
    pub maturity_timestamp: u64,
    /// Whole days remaining until maturity (0 once matured).
    pub days_remaining: u64,
    /// Whether the maturity timestamp has been reached.
    pub is_mature: bool,
    /// Whether settlement has already been triggered.
    pub settlement_triggered: bool,
}
