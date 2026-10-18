#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

#[test]
fn test_partial_settle_pro_rata_and_history() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let contract_id = env.register(PartialSettlement, ());
    let client = PartialSettlementClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = Address::generate(&env);
    let holder1 = Address::generate(&env);
    let holder2 = Address::generate(&env);

    client.initialize(&admin);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let mut holders = Vec::new(&env);
    holders.push_back(HolderShare {
        holder: holder1.clone(),
        share_bps: 7000, // 70%
    });
    holders.push_back(HolderShare {
        holder: holder2.clone(),
        share_bps: 3000, // 30%
    });

    client.register_invoice(&invoice_id, &issuer, &token, &10000, &holders);

    // 1st partial payment of 4000
    env.ledger().set_timestamp(1500);
    let rem1 = client.partial_settle(&invoice_id, &4000);
    assert_eq!(rem1, 6000);

    let state1 = client.get_invoice_state(&invoice_id);
    assert_eq!(state1.cumulative_paid, 4000);
    assert_eq!(state1.remaining_balance, 6000);
    assert_eq!(state1.is_settled, false);

    // 2nd partial payment of 3000
    env.ledger().set_timestamp(2000);
    let rem2 = client.partial_settle(&invoice_id, &3000);
    assert_eq!(rem2, 3000);

    let state2 = client.get_invoice_state(&invoice_id);
    assert_eq!(state2.cumulative_paid, 7000);
    assert_eq!(state2.remaining_balance, 3000);
    assert_eq!(state2.is_settled, false);

    // 3rd final payment of 3000 (settles invoice completely)
    env.ledger().set_timestamp(2500);
    let rem3 = client.partial_settle(&invoice_id, &3000);
    assert_eq!(rem3, 0);

    let state3 = client.get_invoice_state(&invoice_id);
    assert_eq!(state3.cumulative_paid, 10000);
    assert_eq!(state3.remaining_balance, 0);
    assert_eq!(state3.is_settled, true);

    // Verify interim payments history
    let payments = client.get_interim_payments(&invoice_id);
    assert_eq!(payments.len(), 3);
    assert_eq!(payments.get(0).unwrap().amount, 4000);
    assert_eq!(payments.get(1).unwrap().amount, 3000);
    assert_eq!(payments.get(2).unwrap().amount, 3000);
    assert_eq!(payments.get(2).unwrap().remaining_balance, 0);

    // Attempting further payment should fail because already settled
    let res = client.try_partial_settle(&invoice_id, &100);
    assert!(res.is_err());
}
