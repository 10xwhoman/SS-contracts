#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};
pub use types::{BulkItem, BulkRecord, Listing, StorageKey};

const MAX_BULK_ITEMS: u32 = 10;
const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct BulkListing;

#[contractimpl]
impl BulkListing {
    pub fn initialize(
        env: Env,
        token: Address,
        fee_bps: u32,
        fee_recipient: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Token) {
            return Err(Error::AlreadyInitialized);
        }
        if fee_bps as i128 > BPS_DENOMINATOR {
            return Err(Error::InvalidFee);
        }
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&StorageKey::FeeRecipient, &fee_recipient);
        env.storage()
            .instance()
            .set(&StorageKey::NextListingId, &1u32);
        env.storage().instance().set(&StorageKey::NextBulkId, &1u32);
        Ok(())
    }

    /// Creates up to 10 listings atomically. Any failing item reverts the
    /// whole call, and the listing fee for every item is collected at once.
    pub fn bulk_create_listing(
        env: Env,
        seller: Address,
        items: Vec<BulkItem>,
    ) -> Result<u32, Error> {
        seller.require_auth();

        if items.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if items.len() > MAX_BULK_ITEMS {
            return Err(Error::BatchTooLarge);
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::FeeBps)
            .ok_or(Error::NotInitialized)?;
        let fee_recipient: Address = env
            .storage()
            .instance()
            .get(&StorageKey::FeeRecipient)
            .ok_or(Error::NotInitialized)?;

        let bulk_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextBulkId)
            .unwrap_or(1);
        let mut listing_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextListingId)
            .unwrap_or(1);

        let mut listing_ids: Vec<u32> = Vec::new(&env);
        let mut total_fee: i128 = 0;
        for item in items.iter() {
            if item.amount <= 0 {
                return Err(Error::InvalidAmount);
            }
            if item.price <= 0 {
                return Err(Error::InvalidPrice);
            }
            let value = item.amount.checked_mul(item.price).ok_or(Error::Overflow)?;
            let fee = value.checked_mul(fee_bps as i128).ok_or(Error::Overflow)? / BPS_DENOMINATOR;
            total_fee = total_fee.checked_add(fee).ok_or(Error::Overflow)?;

            let listing = Listing {
                listing_id,
                bulk_id,
                seller: seller.clone(),
                invoice_id: item.invoice_id.clone(),
                amount: item.amount,
                price: item.price,
                fee,
            };
            env.storage()
                .persistent()
                .set(&StorageKey::Listing(listing_id), &listing);
            listing_ids.push_back(listing_id);
            listing_id += 1;
        }

        if total_fee > 0 {
            token::Client::new(&env, &token_addr).transfer(&seller, &fee_recipient, &total_fee);
        }

        env.storage()
            .instance()
            .set(&StorageKey::NextListingId, &listing_id);
        env.storage()
            .instance()
            .set(&StorageKey::NextBulkId, &(bulk_id + 1));
        env.storage().persistent().set(
            &StorageKey::Bulk(bulk_id),
            &BulkRecord {
                seller: seller.clone(),
                listing_ids: listing_ids.clone(),
            },
        );

        events::bulk_listing_created(&env, bulk_id, &seller, &listing_ids, &items, total_fee);
        Ok(bulk_id)
    }

    /// Returns every listing created by the given bulk call.
    pub fn get_bulk_listing(env: Env, bulk_id: u32) -> Result<Vec<Listing>, Error> {
        let record: BulkRecord = env
            .storage()
            .persistent()
            .get(&StorageKey::Bulk(bulk_id))
            .ok_or(Error::BulkNotFound)?;

        let mut listings: Vec<Listing> = Vec::new(&env);
        for listing_id in record.listing_ids.iter() {
            let listing: Listing = env
                .storage()
                .persistent()
                .get(&StorageKey::Listing(listing_id))
                .ok_or(Error::BulkNotFound)?;
            listings.push_back(listing);
        }
        Ok(listings)
    }
}
