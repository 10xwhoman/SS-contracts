#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient as AssetClient,
    Address, BytesN, Env, Vec,
};

fn setup_token(env: &Env) -> (Address, AssetClient<'_>) {
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let asset = AssetClient::new(env, &token_id.address());
    (token_id.address(), asset)
}

#[test]
fn test_batch_settle_success_and_prorata() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(2000);

    let contract_id = env.register(BatchSettlement, ());
    let client = BatchSettlementClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    // Fund the settlement contract so pro-rata payouts can transfer.
    asset.mint(&contract_id, &10_000);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);

    client.initialize(&admin, &operator);

    let id1 = BytesN::from_array(&env, &[1u8; 32]);
    let id2 = BytesN::from_array(&env, &[2u8; 32]);

    let mut invs1 = Vec::new(&env);
    invs1.push_back(InvestorShare {
        investor: investor1.clone(),
        share_bps: 6000, // 60%
    });
    invs1.push_back(InvestorShare {
        investor: investor2.clone(),
        share_bps: 4000, // 40%
    });

    let mut invs2 = Vec::new(&env);
    invs2.push_back(InvestorShare {
        investor: investor1.clone(),
        share_bps: 10000, // 100%
    });

    client.register_invoice(&id1, &1000, &1500, &true, &token, &invs1);
    client.register_invoice(&id2, &2000, &1800, &true, &token, &invs2);

    let mut batch = Vec::new(&env);
    batch.push_back(id1.clone());
    batch.push_back(id2.clone());

    let summaries = client.batch_settle(&operator, &batch);
    assert_eq!(summaries.len(), 2);

    let sum1 = summaries.get(0).unwrap();
    assert_eq!(sum1.invoice_id, id1);
    assert_eq!(sum1.total_distributed, 1000);
    assert_eq!(sum1.investor_count, 2);

    let sum2 = summaries.get(1).unwrap();
    assert_eq!(sum2.invoice_id, id2);
    assert_eq!(sum2.total_distributed, 2000);
    assert_eq!(sum2.investor_count, 1);

    // Both should be marked as settled
    let info1 = client.get_invoice_info(&id1);
    assert!(info1.is_settled);
    let info2 = client.get_invoice_info(&id2);
    assert!(info2.is_settled);
}

#[test]
fn test_batch_reverts_if_any_invoice_fails_validation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(2000);

    let contract_id = env.register(BatchSettlement, ());
    let client = BatchSettlementClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &operator);

    let id_valid = BytesN::from_array(&env, &[1u8; 32]);
    let id_unfunded = BytesN::from_array(&env, &[2u8; 32]);

    let mut invs = Vec::new(&env);
    invs.push_back(InvestorShare {
        investor: investor.clone(),
        share_bps: 10000,
    });

    client.register_invoice(&id_valid, &1000, &1500, &true, &token, &invs);
    client.register_invoice(&id_unfunded, &2000, &1500, &false, &token, &invs); // NOT funded!

    let mut batch = Vec::new(&env);
    batch.push_back(id_valid.clone());
    batch.push_back(id_unfunded.clone());

    let res = client.try_batch_settle(&operator, &batch);
    assert!(res.is_err());

    // Ensure first invoice was NOT settled (entire batch reverted)
    let info_valid = client.get_invoice_info(&id_valid);
    assert_eq!(info_valid.is_settled, false);
}

#[test]
fn test_batch_exceeding_20_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchSettlement, ());
    let client = BatchSettlementClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    client.initialize(&admin, &operator);

    let mut batch = Vec::new(&env);
    for i in 0..21 {
        let mut arr = [0u8; 32];
        arr[0] = i as u8;
        batch.push_back(BytesN::from_array(&env, &arr));
    }

    let res = client.try_batch_settle(&operator, &batch);
    assert!(res.is_err());
}
