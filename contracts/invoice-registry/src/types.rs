//! Data types for the invoice registry contract.

use soroban_sdk::{contracttype, BytesN};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: registry admin.
    Admin,
    /// Persistent: category/risk-tier assignment for an invoice.
    Assignment(BytesN<32>),
    /// Persistent: ordered list of invoice IDs assigned to a category.
    CategoryIndex(InvoiceCategory),
    /// Instance: contract allowed to flag invoices as under review.
    ReviewAuthority,
    /// Persistent: under-review flag for an invoice.
    UnderReview(BytesN<32>),
}

/// Invoice sector classification tracked by the registry.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceCategory {
    /// Standard trade credit / trade finance.
    TradeFinance = 0,
    /// Real estate backed invoices.
    RealEstate = 1,
    /// Small and medium enterprise invoices.
    Sme = 2,
    /// Infrastructure project invoices.
    Infrastructure = 3,
}

/// Risk tier assigned to an invoice.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RiskTier {
    /// Lowest risk.
    Low = 0,
    /// Moderate risk.
    Medium = 1,
    /// Highest risk.
    High = 2,
}

/// Full classification returned by `get_category`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryAssignment {
    /// Invoice the classification belongs to.
    pub invoice_id: BytesN<32>,
    /// Invoice sector classification.
    pub category: InvoiceCategory,
    /// Assigned risk tier.
    pub risk_tier: RiskTier,
}
