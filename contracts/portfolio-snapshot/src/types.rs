//! Data types for the portfolio snapshot contract.

use soroban_sdk::{contracttype, Address, Map, Symbol};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: snapshot admin.
    Admin,
    /// Persistent: registered invoice-token contracts.
    Tokens,
    /// Persistent: next snapshot id for a wallet (1-based).
    NextSnapshotId(Address),
    /// Persistent: ordered snapshot index for a wallet.
    SnapshotList(Address),
    /// Persistent: snapshot payload by (wallet, snapshot_id).
    Snapshot(Address, u64),
}

/// A registered invoice-token contract tracked by the snapshot aggregator.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenEntry {
    /// Invoice identifier the token contract represents.
    pub invoice_id: Symbol,
    /// Invoice token contract address (SEP-41 fraction token).
    pub token: Address,
}

/// Lightweight snapshot index entry returned by `list_snapshots`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotInfo {
    /// Monotonic, per-wallet snapshot identifier (starting at 1).
    pub snapshot_id: u64,
    /// Ledger timestamp the snapshot was taken at.
    pub timestamp: u64,
}

/// Full portfolio snapshot: every non-zero invoice fraction balance held by
/// the wallet across all registered invoice-token contracts at one instant.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotData {
    /// Monotonic, per-wallet snapshot identifier.
    pub snapshot_id: u64,
    /// Ledger timestamp the snapshot was taken at.
    pub timestamp: u64,
    /// Fraction balances keyed by invoice id (zero balances omitted).
    pub balances: Map<Symbol, i128>,
}
