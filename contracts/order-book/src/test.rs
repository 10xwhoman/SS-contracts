#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env,
};

#[test]
fn test_place_buy_order_and_cancel() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(OrderBook, ());
    let client = OrderBookClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let buyer = Address::generate(&env);

    client.initialize(&admin, &token);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Place buy order: 10 fractions at max 100 stroops = 1000 escrow
    let order_id = client.place_buy_order(&buyer, &invoice_id, &10, &100);
    assert_eq!(order_id, 1);

    let order = client.get_order(&order_id);
    assert_eq!(order.fraction_amount, 10);
    assert_eq!(order.max_price_per_fraction, 100);
    assert_eq!(order.escrowed_funds, 1000);
    assert!(order.is_active);

    // Open orders should have 1 item
    let open_orders = client.get_open_orders(&invoice_id);
    assert_eq!(open_orders.len(), 1);

    // Cancel order
    let refund = client.cancel_order(&buyer, &order_id);
    assert_eq!(refund, 1000);

    let order_after = client.get_order(&order_id);
    assert_eq!(order_after.is_active, false);

    // Open orders should now be empty
    let open_after = client.get_open_orders(&invoice_id);
    assert_eq!(open_after.len(), 0);
}

#[test]
fn test_match_listing_and_price_sorting() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(OrderBook, ());
    let client = OrderBookClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let seller = Address::generate(&env);

    client.initialize(&admin, &token);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Buyer 1 bids 5 fractions at max 80
    client.place_buy_order(&buyer1, &invoice_id, &5, &80);
    // Buyer 2 bids 5 fractions at max 100 (higher bid!)
    client.place_buy_order(&buyer2, &invoice_id, &5, &100);

    // Open orders should be sorted descending by price (buyer 2 at 100 first, then buyer 1 at 80)
    let open_orders = client.get_open_orders(&invoice_id);
    assert_eq!(open_orders.len(), 2);
    assert_eq!(open_orders.get(0).unwrap().max_price_per_fraction, 100);
    assert_eq!(open_orders.get(1).unwrap().max_price_per_fraction, 80);

    // Seller lists 3 fractions at ask price 90
    // Buyer 2 matches at 90 (surplus refunded: 100 - 90 = 10 per fraction)
    // Buyer 1 (max 80) does NOT match (price too low)
    let matches = client.match_listing(&seller, &invoice_id, &3, &90);
    assert_eq!(matches.len(), 1);

    let m = matches.get(0).unwrap();
    assert_eq!(m.buyer, buyer2);
    assert_eq!(m.seller, seller);
    assert_eq!(m.matched_amount, 3);
    assert_eq!(m.execution_price, 90);
    assert_eq!(m.total_cost, 270);

    // Buyer 2 still has 2 fractions open at 100
    let open_after = client.get_open_orders(&invoice_id);
    assert_eq!(open_after.len(), 2);
    assert_eq!(open_after.get(0).unwrap().fraction_amount, 2);
    assert_eq!(open_after.get(1).unwrap().fraction_amount, 5);
}

#[test]
fn test_unmatched_orders_remain_open_when_ask_too_high() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(OrderBook, ());
    let client = OrderBookClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    client.initialize(&admin, &token);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Buyer bids at max 50
    client.place_buy_order(&buyer, &invoice_id, &10, &50);

    // Seller asks for 70 (> 50)
    let matches = client.match_listing(&seller, &invoice_id, &5, &70);
    assert_eq!(matches.len(), 0);

    // Order remains open
    let open_orders = client.get_open_orders(&invoice_id);
    assert_eq!(open_orders.len(), 1);
    assert_eq!(open_orders.get(0).unwrap().fraction_amount, 10);
}
