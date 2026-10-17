//! Data types for the secondary market fee contract.

use soroban_sdk::contracttype;

/// Storage key enum for instance storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: fee contract admin.
    Admin,
    /// Instance: treasury contract (fee destination).
    Treasury,
    /// Instance: platform fee rate in basis points.
    FeeBps,
    /// Instance: authorized secondary market contract.
    SecondaryMarket,
}

/// Result of a fee collection: the sale split between seller and treasury.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeReceipt {
    /// Gross sale amount the fee was computed from.
    pub sale_amount: i128,
    /// Amount routed to the seller (`sale_amount - fee_amount`).
    pub net_amount: i128,
    /// Platform fee routed to the treasury.
    pub fee_amount: i128,
}
