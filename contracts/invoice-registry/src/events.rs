#![allow(deprecated)]
//! Events emitted by the invoice registry contract.

use soroban_sdk::{BytesN, Env, Symbol};

use crate::types::{InvoiceCategory, RiskTier};

/// Emitted when the registry is initialized.
pub fn initialized(env: &Env, admin: &soroban_sdk::Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted when an invoice's category and risk tier are set or updated.
/// Topics: (CategoryAssigned, invoice_id); Data: (category, risk_tier).
pub fn category_assigned(
    env: &Env,
    invoice_id: &BytesN<32>,
    category: InvoiceCategory,
    risk_tier: RiskTier,
) {
    let topics = (
        Symbol::new(env, "CategoryAssigned"),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (category as u32, risk_tier as u32));
}
