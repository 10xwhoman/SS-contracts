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
fn test_initialize_and_create_vest() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 1000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 2000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);
    assert_eq!(vest_id, 1);

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.total_amount, 1000);
    assert_eq!(schedule.claimed_amount, 0);
    assert_eq!(schedule.tranches.len(), 2);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, false);
}

#[test]
fn test_claim_tranche_blocked_before_release_time() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(500);

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 1000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Should fail with TimeNotReached
    let res = client.try_claim_tranche(&vest_id, &0);
    assert!(res.is_err());
}

#[test]
fn test_claim_tranche_success_after_release_time() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    asset.mint(&contract_id, &10_000);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 600,
        release_timestamp: 1000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Fast-forward timestamp past release
    env.ledger().set_timestamp(1500);

    let claimed = client.claim_tranche(&vest_id, &0);
    assert_eq!(claimed, 600);

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.claimed_amount, 600);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, true);

    // Attempting to claim again should fail
    let res2 = client.try_claim_tranche(&vest_id, &0);
    assert!(res2.is_err());
}

#[test]
fn test_batch_claim_and_claim_all_eligible() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    asset.mint(&contract_id, &10_000);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 200,
        release_timestamp: 1000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 300,
        release_timestamp: 2000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 5000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Advance to 2500 -> first 2 tranches eligible, third is not
    env.ledger().set_timestamp(2500);

    let claimed_total = client.claim_all_eligible_tranches(&vest_id);
    assert_eq!(claimed_total, 500); // 200 + 300

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.claimed_amount, 500);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, true);
    assert_eq!(schedule.tranches.get(1).unwrap().claimed, true);
    assert_eq!(schedule.tranches.get(2).unwrap().claimed, false);
}
