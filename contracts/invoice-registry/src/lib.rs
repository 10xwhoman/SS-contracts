//! Invoice Registry contract for StellarSettle.
//!
//! Stores per-invoice category classifications and risk tier assignments.
//! The accreditation contract (or any other eligibility checker) can consult
//! the registry via `get_category` / `get_risk_tier` before allowing an
//! investor to participate in an invoice.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{CategoryAssignment, InvoiceCategory, RiskTier};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

use errors::Error;

/// Hard cap on the number of invoices indexed per category. Bounds storage
/// growth and keeps `get_invoices_by_category` iteration affordable.
const MAX_INDEX_ENTRIES: u32 = 500;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

/// Return a copy of `index` without `invoice_id` (order preserved).
fn without_invoice(
    env: &Env,
    index: &Vec<BytesN<32>>,
    invoice_id: &BytesN<32>,
) -> Vec<BytesN<32>> {
    let mut out = Vec::new(env);
    for entry in index.iter() {
        if entry != *invoice_id {
            out.push_back(entry);
        }
    }
    out
}

#[contract]
pub struct InvoiceRegistry;

#[contractimpl]
impl InvoiceRegistry {
    /// Initialize the registry with the admin allowed to classify invoices.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: assign (or re-assign) a category and risk tier to an invoice.
    ///
    /// Re-classifying an invoice moves it between category indexes rather than
    /// duplicating it, so `get_invoices_by_category` always reflects the current
    /// classification.
    pub fn set_category(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        category: InvoiceCategory,
        risk_tier: RiskTier,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let existing = storage::get_assignment(&env, &invoice_id);
        match existing {
            Some(previous) if previous.category != category => {
                // Move the invoice out of its old category index.
                let old_index = storage::get_index(&env, previous.category);
                let trimmed = without_invoice(&env, &old_index, &invoice_id);
                storage::set_index(&env, previous.category, &trimmed);

                let mut new_index = storage::get_index(&env, category);
                if new_index.len() >= MAX_INDEX_ENTRIES {
                    return Err(Error::CategoryIndexFull);
                }
                new_index.push_back(invoice_id.clone());
                storage::set_index(&env, category, &new_index);
            }
            Some(_) => {
                // Same category: only the risk tier changes.
            }
            None => {
                let mut index = storage::get_index(&env, category);
                if index.len() >= MAX_INDEX_ENTRIES {
                    return Err(Error::CategoryIndexFull);
                }
                index.push_back(invoice_id.clone());
                storage::set_index(&env, category, &index);
            }
        }

        let assignment = CategoryAssignment {
            invoice_id: invoice_id.clone(),
            category,
            risk_tier,
        };
        storage::set_assignment(&env, &invoice_id, &assignment);
        events::category_assigned(&env, &invoice_id, category, risk_tier);
        Ok(())
    }

    /// View: return the category and risk tier assigned to an invoice.
    pub fn get_category(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<CategoryAssignment, Error> {
        storage::get_assignment(&env, &invoice_id).ok_or(Error::NotRegistered)
    }

    /// View: return the risk tier assigned to an invoice.
    pub fn get_risk_tier(env: Env, invoice_id: BytesN<32>) -> Result<RiskTier, Error> {
        Ok(storage::get_assignment(&env, &invoice_id)
            .ok_or(Error::NotRegistered)?
            .risk_tier)
    }

    /// View: return every invoice assigned to `category`, in insertion order.
    pub fn get_invoices_by_category(env: Env, category: InvoiceCategory) -> Vec<BytesN<32>> {
        storage::get_index(&env, category)
    }

    /// Admin-only: remove an invoice classification and its index entry.
    pub fn remove_category(env: Env, admin: Address, invoice_id: BytesN<32>) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let assignment =
            storage::get_assignment(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        let index = storage::get_index(&env, assignment.category);
        let trimmed = without_invoice(&env, &index, &invoice_id);
        storage::set_index(&env, assignment.category, &trimmed);
        storage::remove_assignment(&env, &invoice_id);
        Ok(())
    }

    /// Admin-only: bind the contract (e.g. the fraud registry) allowed to flag
    /// invoices as under review.
    pub fn set_review_authority(
        env: Env,
        admin: Address,
        authority: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        ensure_non_zero_address(&env, &authority)?;
        admin.require_auth();
        storage::set_review_authority(&env, &authority);
        Ok(())
    }

    /// Review-authority-only: flag an invoice as under review.
    pub fn mark_under_review(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        if storage::get_review_authority(&env) != Some(caller.clone()) {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        storage::set_under_review(&env, &invoice_id, true);
        events::invoice_under_review(&env, &invoice_id, &caller);
        Ok(())
    }

    /// Admin-only: clear an invoice's under-review flag.
    pub fn clear_under_review(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        storage::set_under_review(&env, &invoice_id, false);
        events::invoice_review_cleared(&env, &invoice_id);
        Ok(())
    }

    /// View: whether an invoice is flagged as under review.
    pub fn is_under_review(env: Env, invoice_id: BytesN<32>) -> bool {
        storage::is_under_review(&env, &invoice_id)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
