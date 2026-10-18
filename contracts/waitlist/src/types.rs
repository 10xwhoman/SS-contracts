//! Data types for the waitlist contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config.
    Config,
    /// Instance: next available queue slot number (starts at 1).
    NextSlot,
    /// Persistent: wallet at a given queue position (1-indexed).
    QueueSlot(u32),
    /// Persistent: queue position for a given wallet (0 = not in queue).
    WalletPos(soroban_sdk::Address),
    /// Persistent: email hash for a given wallet.
    WalletEml(soroban_sdk::Address),
    /// Persistent: whether a wallet has been approved (promoted to KYC whitelist).
    Approved(soroban_sdk::Address),
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address (same admin as the escrow/KYC whitelist contract).
    pub admin: soroban_sdk::Address,
    /// The KYC whitelist contract (invoice-escrow) to promote approved wallets into.
    pub escrow_contract: soroban_sdk::Address,
}
