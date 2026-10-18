//! Data types for the invoice refund contract.
//! Field names respect Soroban's symbol length limits for `contracttype`.

use soroban_sdk::{contracttype, Address, Symbol, Vec};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config (admin + USDC token).
    Config,
    /// Persistent: refund invoice record by invoice id.
    Invoice(Symbol),
    /// Persistent: locked amount by (invoice_id, investor).
    Investment(Symbol, Address),
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin authorized to register invoices and update configuration.
    pub admin: Address,
    /// USDC (or USDC-equivalent) payment token escrowed for refunds.
    pub usdc_token: Address,
}

/// Registered invoice awaiting funding, with a refund fallback on expiry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundInvoice {
    /// Invoice identifier.
    pub invoice_id: Symbol,
    /// Funding threshold: the amount that must be raised for the invoice to
    /// proceed. Expiry below this threshold makes every position refundable.
    pub funding_threshold: i128,
    /// Total amount currently locked by investors.
    pub total_locked: i128,
    /// Expiry timestamp (ledger seconds). Once `now >= expiry` and the
    /// threshold was not reached, `trigger_refund` becomes callable.
    pub expiry_timestamp: u64,
    /// Investors with a non-zero locked position, in first-deposit order.
    pub investors: Vec<Address>,
    /// Set once `trigger_refund` completes; re-triggers are rejected.
    pub refunded: bool,
    /// Ledger timestamp at registration.
    pub created_at: u64,
}

/// Result of a refund sweep, returned by `trigger_refund`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundSummary {
    /// Invoice that was refunded.
    pub invoice_id: Symbol,
    /// Total amount returned across all investors.
    pub total_refunded: i128,
    /// Number of investor positions refunded.
    pub investor_count: u32,
    /// Ledger timestamp the refund completed at.
    pub refunded_at: u64,
}
