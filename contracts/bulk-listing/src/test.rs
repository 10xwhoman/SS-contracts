#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    vec, Address, BytesN, Env,
};

struct Ctx<'a> {
    client: BulkListingClient<'a>,
    token: Address,
    recipient: Address,
}

fn setup(env: &Env, fee_bps: u32) -> Ctx<'_> {
    env.mock_all_auths();
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let recipient = Address::generate(env);
    let contract_id = env.register(BulkListing, ());
    let client = BulkListingClient::new(env, &contract_id);
    client.initialize(&token, &fee_bps, &recipient);
    Ctx {
        client,
        token,
        recipient,
    }
}

fn item(env: &Env, seed: u8, amount: i128, price: i128) -> BulkItem {
    BulkItem {
        invoice_id: BytesN::from_array(env, &[seed; 32]),
        amount,
        price,
    }
}

#[test]
fn test_bulk_create_collects_fees_and_returns_listings() {
    let env = Env::default();
    let c = setup(&env, 100); // 1%
    let seller = Address::generate(&env);
    AssetClient::new(&env, &c.token).mint(&seller, &1_000);

    let items = vec![&env, item(&env, 1, 10, 100), item(&env, 2, 5, 200)];
    let bulk_id = c.client.bulk_create_listing(&seller, &items);
    assert_eq!(bulk_id, 1);

    // fees: 1000 * 1% = 10 and 1000 * 1% = 10
    let token = TokenClient::new(&env, &c.token);
    assert_eq!(token.balance(&c.recipient), 20);
    assert_eq!(token.balance(&seller), 980);

    let listings = c.client.get_bulk_listing(&bulk_id);
    assert_eq!(listings.len(), 2);
    let first = listings.get(0).unwrap();
    assert_eq!(first.listing_id, 1);
    assert_eq!(first.bulk_id, 1);
    assert_eq!(first.seller, seller);
    assert_eq!(first.amount, 10);
    assert_eq!(first.price, 100);
    assert_eq!(first.fee, 10);
    assert_eq!(listings.get(1).unwrap().listing_id, 2);

    // a second bulk call continues the listing id sequence
    let bulk2 = c
        .client
        .bulk_create_listing(&seller, &vec![&env, item(&env, 3, 1, 100)]);
    assert_eq!(bulk2, 2);
    assert_eq!(
        c.client.get_bulk_listing(&bulk2).get(0).unwrap().listing_id,
        3
    );
}

#[test]
fn test_batch_over_ten_items_rejected() {
    let env = Env::default();
    let c = setup(&env, 0);
    let seller = Address::generate(&env);

    let mut items = Vec::new(&env);
    for i in 0..11u8 {
        items.push_back(item(&env, i, 1, 1));
    }
    assert_eq!(
        c.client.try_bulk_create_listing(&seller, &items),
        Err(Ok(Error::BatchTooLarge))
    );

    let mut ten = Vec::new(&env);
    for i in 0..10u8 {
        ten.push_back(item(&env, i, 1, 1));
    }
    assert_eq!(c.client.bulk_create_listing(&seller, &ten), 1);
    assert_eq!(c.client.get_bulk_listing(&1).len(), 10);
}

#[test]
fn test_invalid_item_reverts_whole_batch() {
    let env = Env::default();
    let c = setup(&env, 100);
    let seller = Address::generate(&env);
    AssetClient::new(&env, &c.token).mint(&seller, &1_000);

    let items = vec![&env, item(&env, 1, 10, 100), item(&env, 2, 0, 200)];
    assert_eq!(
        c.client.try_bulk_create_listing(&seller, &items),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(TokenClient::new(&env, &c.token).balance(&seller), 1_000);
    assert_eq!(
        c.client.try_get_bulk_listing(&1),
        Err(Ok(Error::BulkNotFound))
    );
    assert_eq!(
        c.client.try_bulk_create_listing(&seller, &Vec::new(&env)),
        Err(Ok(Error::EmptyBatch))
    );
}

#[test]
fn test_bulk_created_event_emitted() {
    let env = Env::default();
    let c = setup(&env, 0);
    let seller = Address::generate(&env);

    c.client
        .bulk_create_listing(&seller, &vec![&env, item(&env, 1, 1, 1)]);
    assert_eq!(env.events().all().events().len(), 1);
}
