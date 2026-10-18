//! Data types for the invoice token contract (SEP-41).
//! Storage key names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::{contracttype, xdr::ToXdr, Address, Env};

/// Largest supported number of fractional digits for an invoice sub-asset.
pub const MAX_DECIMALS: u32 = 18;

/// Return whether an address has an all-zero account or contract payload.
pub fn is_zero_address(env: &Env, address: &Address) -> bool {
    let encoded = address.to_xdr(env);
    let (payload_start, payload_end) = match encoded.len() {
        // ScVal::Address + ScAddress::Account + PublicKey::Ed25519.
        44 => (12, 44),
        // ScVal::Address + ScAddress::Contract.
        40 => (8, 40),
        _ => return false,
    };

    (payload_start..payload_end).all(|index| encoded.get(index) == Some(0))
}

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: token metadata and config.
    Metadata,
    /// Instance: total supply.
    TotalSupply,
    /// Persistent: balance by holder address.
    Balance(soroban_sdk::Address),
    /// Persistent: allowance (from, spender) -> AllowanceData.
    Allowance(soroban_sdk::Address, soroban_sdk::Address),
    /// Instance: fee basis points.
    FeeBps,
    /// Instance: role admin mapping (role -> admin address).
    RoleAdmin(soroban_sdk::Symbol),
    /// Instance: role grant mapping (role, account) -> bool.
    RoleGrant(soroban_sdk::Symbol, soroban_sdk::Address),
    /// Persistent: nonce per address for permit-style transfers.
    Nonce(soroban_sdk::Address),
    /// Persistent: ownership history records for a token holder.
    History(soroban_sdk::Address),
    /// Persistent: whether an account is restricted from token operations.
    Frozen(soroban_sdk::Address),
}

/// Token metadata and admin config (instance storage).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    /// Admin address (can mint, set minter, set transfer_locked).
    pub admin: soroban_sdk::Address,
    /// Escrow contract (or minter) address allowed to mint.
    pub minter: soroban_sdk::Address,
    /// Token name (e.g. "Invoice INV-001 Token").
    pub name: soroban_sdk::String,
    /// Token symbol (e.g. "INV001").
    pub symbol: soroban_sdk::String,
    /// Number of decimals (e.g. 7).
    pub decimals: u32,
    /// Invoice identifier this token represents (Symbol for storage efficiency).
    pub invoice_id: soroban_sdk::Symbol,
    /// If true, transfers restricted until settlement (admin can still transfer).
    pub transfer_locked: bool,
    /// Emergency pause flag for sensitive token operations.
    pub paused: bool,
    /// Optional lock-up contract consulted before holder transfers.
    pub transfer_restriction: Option<soroban_sdk::Address>,
}

/// Allowance entry: amount and expiration ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceData {
    pub amount: i128,
    pub expiration_ledger: u32,
}

/// A single ownership history record for a token holder.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipHistoryRecord {
    /// The previous owner before this transfer.
    pub from: soroban_sdk::Address,
    /// The new owner (recipient) of the tokens.
    pub to: soroban_sdk::Address,
    /// Amount of tokens transferred in this event.
    pub amount: i128,
    /// Ledger sequence at the time of the transfer.
    pub ledger: u32,
}
