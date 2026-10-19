//! Data types for the secondary market fee contract.

use soroban_sdk::{contracttype, BytesN};

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
    /// Instance: invoice registry used to resolve an invoice's sector.
    InvoiceRegistry,
    /// Instance: flat listing fee per fraction listed, by sector.
    ListingFee(Sector),
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

/// Invoice sector, mirroring the invoice registry's category enum so the
/// registry's `get_category` response can be decoded here.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Sector {
    TradeFinance = 0,
    RealEstate = 1,
    Sme = 2,
    Infrastructure = 3,
}

/// Mirror of the invoice registry's `CategoryAssignment` response.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryAssignment {
    /// Invoice the classification belongs to.
    pub invoice_id: BytesN<32>,
    /// Invoice sector classification.
    pub category: Sector,
    /// Risk tier discriminant (unused by the fee contract).
    pub risk_tier: u32,
}
