//! Data types for the investor position snapshot contract.

use soroban_sdk::{contracttype, Address, Symbol, Vec};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: the admin authorized to update balances and take snapshots.
    Admin,
    /// Persistent: ordered list of holders for an invoice.
    Holders(Symbol),
    /// Persistent: current balance by (invoice_id, holder).
    HolderBalance(Symbol, Address),
    /// Persistent: next snapshot id to assign for an invoice.
    NextSnapshotId(Symbol),
    /// Persistent: a snapshot by (invoice_id, snapshot_id).
    Snapshot(Symbol, u32),
    /// Persistent: ordered live snapshot ids per invoice.
    SnapshotIndex(Symbol),
    /// Persistent: per-wallet references across every invoice, oldest first.
    WalletIndex(Address),
}

/// One holder's balance inside a snapshot's balance map.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HolderBalance {
    /// Investor wallet holding the position.
    pub holder: Address,
    /// Recorded balance at snapshot time.
    pub amount: i128,
}

/// A full invoice balance map captured at one ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Sequential id within the invoice (starts at 1).
    pub snapshot_id: u32,
    /// Invoice this snapshot belongs to.
    pub invoice_id: Symbol,
    /// Ledger sequence (block height) the snapshot was taken at.
    pub ledger_sequence: u32,
    /// Ledger timestamp the snapshot was taken at.
    pub timestamp: u64,
    /// Every holder with a non-zero balance at snapshot time.
    pub balances: Vec<HolderBalance>,
    /// Sum of all recorded balances.
    pub total_balance: i128,
}

/// A per-wallet reference to one snapshot entry, used by
/// `get_investor_snapshots` to reconstruct portfolio history across invoices.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorSnapshot {
    /// Invoice the snapshot was taken for.
    pub invoice_id: Symbol,
    /// Snapshot id within that invoice.
    pub snapshot_id: u32,
    /// Ledger sequence (block height) of the snapshot.
    pub ledger_sequence: u32,
    /// Ledger timestamp of the snapshot.
    pub timestamp: u64,
    /// The wallet's balance in that snapshot.
    pub balance: i128,
}

/// A lightweight reference to one live snapshot, returned by `list_snapshots`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRef {
    /// Snapshot id within the invoice.
    pub snapshot_id: u32,
    /// Ledger timestamp the snapshot was taken at.
    pub timestamp: u64,
}
