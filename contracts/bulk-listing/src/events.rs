use crate::types::BulkItem;
use soroban_sdk::{Address, Env, Symbol, Vec};

pub fn bulk_listing_created(
    env: &Env,
    bulk_id: u32,
    seller: &Address,
    listing_ids: &Vec<u32>,
    items: &Vec<BulkItem>,
    total_fee: i128,
) {
    let topics = (
        Symbol::new(env, "BulkListingCreated"),
        bulk_id,
        seller.clone(),
    );
    env.events()
        .publish(topics, (listing_ids.clone(), items.clone(), total_fee));
}
