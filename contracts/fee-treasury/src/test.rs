#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env,
};

#[test]
fn test_fee_collection_and_split() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FeeTreasury, ());
    let client = FeeTreasuryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = Address::generate(&env);
    let protocol = Address::generate(&env);
    let lp_pool = Address::generate(&env);
    let issuer = Address::generate(&env);

    client.initialize(&admin, &operator, &token, &protocol, &lp_pool, &None);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_invoice_issuer(&invoice_id, &issuer);

    // Collect fee of 1000: 50% protocol (500), 30% issuer (300), 20% lp_pool (200)
    let splits = client.collect_fee(&operator, &invoice_id, &1000);
    assert_eq!(splits.protocol_amount, 500);
    assert_eq!(splits.issuer_amount, 300);
    assert_eq!(splits.lp_pool_amount, 200);

    // Verify balances
    assert_eq!(client.get_balance(&protocol), 500);
    assert_eq!(client.get_balance(&issuer), 300);
    assert_eq!(client.get_balance(&lp_pool), 200);

    // Withdraw issuer balance
    let withdrawn = client.withdraw(&issuer);
    assert_eq!(withdrawn, 300);
    assert_eq!(client.get_balance(&issuer), 0);
}

#[test]
fn test_update_split_ratios() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FeeTreasury, ());
    let client = FeeTreasuryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = Address::generate(&env);
    let protocol = Address::generate(&env);
    let lp_pool = Address::generate(&env);
    let issuer = Address::generate(&env);

    client.initialize(&admin, &operator, &token, &protocol, &lp_pool, &None);

    // Admin updates split to 40% protocol, 40% issuer, 20% LP
    client.update_split_ratios(&SplitRatios {
        protocol_bps: 4000,
        issuer_bps: 4000,
        lp_pool_bps: 2000,
    });

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_invoice_issuer(&invoice_id, &issuer);

    let splits = client.collect_fee(&operator, &invoice_id, &1000);
    assert_eq!(splits.protocol_amount, 400);
    assert_eq!(splits.issuer_amount, 400);
    assert_eq!(splits.lp_pool_amount, 200);
}

#[test]
fn test_invalid_split_ratios_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FeeTreasury, ());
    let client = FeeTreasuryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = Address::generate(&env);
    let protocol = Address::generate(&env);
    let lp_pool = Address::generate(&env);

    client.initialize(&admin, &operator, &token, &protocol, &lp_pool, &None);

    // Sum is 9000 != 10000 -> Should fail
    let res = client.try_update_split_ratios(&SplitRatios {
        protocol_bps: 3000,
        issuer_bps: 3000,
        lp_pool_bps: 3000,
    });
    assert!(res.is_err());
}
