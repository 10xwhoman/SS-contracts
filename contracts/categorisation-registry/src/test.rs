#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String,
};

#[test]
fn test_register_and_get_category() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(12345);

    let contract_id = env.register(CategorisationRegistry, ());
    let client = CategorisationRegistryClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let sub_category = String::from_str(&env, "Semiconductors");

    // Sector 4 is Technology
    client.register_category(&issuer, &invoice_id, &4, &sub_category);

    let info = client.get_category(&invoice_id);
    assert_eq!(info.invoice_id, invoice_id);
    assert_eq!(info.issuer, issuer);
    assert_eq!(info.sector, Sector::Technology);
    assert_eq!(info.sub_category, sub_category);
    assert_eq!(info.registered_at, 12345);
}

#[test]
fn test_invalid_sector_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CategorisationRegistry, ());
    let client = CategorisationRegistryClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let sub_category = String::from_str(&env, "Automotive");

    // Sector 99 is invalid
    let res = client.try_register_category(&issuer, &invoice_id, &99, &sub_category);
    assert!(res.is_err());
}

#[test]
fn test_list_by_sector_pagination() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CategorisationRegistry, ());
    let client = CategorisationRegistryClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);

    // Register 5 invoices in Sector 0 (Manufacturing)
    for i in 0..5 {
        let mut arr = [0u8; 32];
        arr[0] = i as u8;
        let id = BytesN::from_array(&env, &arr);
        let sub = String::from_str(&env, "Hardware");
        client.register_category(&issuer, &id, &0, &sub);
    }

    // Page 0, page_size 2
    let page0 = client.list_by_sector(&0, &0, &2);
    assert_eq!(page0.len(), 2);

    // Page 1, page_size 2
    let page1 = client.list_by_sector(&0, &1, &2);
    assert_eq!(page1.len(), 2);

    // Page 2, page_size 2
    let page2 = client.list_by_sector(&0, &2, &2);
    assert_eq!(page2.len(), 1);

    // Page 3 out of bounds
    let page3 = client.list_by_sector(&0, &3, &2);
    assert_eq!(page3.len(), 0);
}
