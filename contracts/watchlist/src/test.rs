#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env,
};

#[test]
fn test_add_remove_and_get_watchlist() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Watchlist, ());
    let client = WatchlistClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &operator);

    let id1 = BytesN::from_array(&env, &[1u8; 32]);
    let id2 = BytesN::from_array(&env, &[2u8; 32]);
    let id3 = BytesN::from_array(&env, &[3u8; 32]);

    client.add_to_watchlist(&investor, &id1);
    client.add_to_watchlist(&investor, &id2);
    client.add_to_watchlist(&investor, &id3);

    // Pagination: page 0, size 2
    let page0 = client.get_watchlist(&investor, &0, &2);
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0).unwrap(), id1);
    assert_eq!(page0.get(1).unwrap(), id2);

    // Page 1, size 2
    let page1 = client.get_watchlist(&investor, &1, &2);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap(), id3);

    // Remove id2
    client.remove_from_watchlist(&investor, &id2);

    let all_after = client.get_watchlist(&investor, &0, &10);
    assert_eq!(all_after.len(), 2);
    assert_eq!(all_after.get(0).unwrap(), id1);
    assert_eq!(all_after.get(1).unwrap(), id3);
}

#[test]
fn test_watchlist_full_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Watchlist, ());
    let client = WatchlistClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &operator);

    // Add 50 invoices
    for i in 0..50 {
        let mut arr = [0u8; 32];
        arr[0] = i as u8;
        let id = BytesN::from_array(&env, &arr);
        client.add_to_watchlist(&investor, &id);
    }

    // Adding 51st should fail with WatchlistFull
    let mut arr51 = [0u8; 32];
    arr51[0] = 51;
    let id51 = BytesN::from_array(&env, &arr51);
    let res = client.try_add_to_watchlist(&investor, &id51);
    assert!(res.is_err());
}

#[test]
fn test_notify_state_change_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Watchlist, ());
    let client = WatchlistClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    client.initialize(&admin, &operator);

    let id = BytesN::from_array(&env, &[1u8; 32]);
    client.add_to_watchlist(&investor1, &id);
    client.add_to_watchlist(&investor2, &id);

    // Notify state change from state 0 (Created) to state 1 (Funded)
    let notified_count = client.notify_state_change(&operator, &id, &0, &1);
    assert_eq!(notified_count, 2);
}
