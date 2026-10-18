#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    vec, Address, BytesN, Env,
};

fn setup(env: &Env) -> (InvoicePoolClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract_id = env.register(InvoicePool, ());
    let client = InvoicePoolClient::new(env, &contract_id);
    client.initialize(&admin, &token);
    (client, token)
}

fn ids(env: &Env) -> Vec<BytesN<32>> {
    vec![
        env,
        BytesN::from_array(env, &[1u8; 32]),
        BytesN::from_array(env, &[2u8; 32]),
        BytesN::from_array(env, &[3u8; 32]),
    ]
}

#[test]
fn test_create_pool_and_invest_pro_rata() {
    let env = Env::default();
    let (client, token) = setup(&env);
    let investor = Address::generate(&env);
    AssetClient::new(&env, &token).mint(&investor, &10_000);

    let pool_id = client.create_pool(&ids(&env), &vec![&env, 5_000u32, 3_000, 2_000]);
    assert_eq!(pool_id, 1);
    assert_eq!(env.events().all().events().len(), 1);

    client.invest_pool(&investor, &pool_id, &1_000);

    let comp = client.get_pool_composition(&pool_id);
    assert_eq!(comp.tvl, 1_000);
    assert_eq!(comp.allocations, vec![&env, 500i128, 300, 200]);
    assert_eq!(comp.weights, vec![&env, 5_000u32, 3_000, 2_000]);
    assert_eq!(comp.invoice_ids, ids(&env));
    assert_eq!(client.get_position(&pool_id, &investor), 1_000);
    assert_eq!(
        TokenClient::new(&env, &token).balance(&client.address),
        1_000
    );
}

#[test]
fn test_rounding_remainder_goes_to_last_invoice() {
    let env = Env::default();
    let (client, token) = setup(&env);
    let investor = Address::generate(&env);
    AssetClient::new(&env, &token).mint(&investor, &100);

    let pool_id = client.create_pool(&ids(&env), &vec![&env, 3_333u32, 3_333, 3_334]);
    client.invest_pool(&investor, &pool_id, &10);

    let comp = client.get_pool_composition(&pool_id);
    assert_eq!(comp.allocations, vec![&env, 3i128, 3, 4]);
    assert_eq!(comp.tvl, 10);
}

#[test]
fn test_weights_must_sum_to_10000() {
    let env = Env::default();
    let (client, _) = setup(&env);

    assert_eq!(
        client.try_create_pool(&ids(&env), &vec![&env, 5_000u32, 3_000, 1_000]),
        Err(Ok(Error::InvalidWeights))
    );
    assert_eq!(
        client.try_create_pool(&ids(&env), &vec![&env, 5_000u32, 5_000]),
        Err(Ok(Error::InvalidPool))
    );
}

#[test]
fn test_pool_yield_is_weighted_average() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let invoice_ids = ids(&env);
    let pool_id = client.create_pool(&invoice_ids, &vec![&env, 5_000u32, 3_000, 2_000]);

    assert_eq!(
        client.try_get_pool_yield(&pool_id),
        Err(Ok(Error::YieldNotSet))
    );

    client.set_invoice_yield(&invoice_ids.get(0).unwrap(), &1_000);
    client.set_invoice_yield(&invoice_ids.get(1).unwrap(), &500);
    client.set_invoice_yield(&invoice_ids.get(2).unwrap(), &2_000);

    // 0.5 * 1000 + 0.3 * 500 + 0.2 * 2000 = 1050
    assert_eq!(client.get_pool_yield(&pool_id), 1_050);
}

#[test]
fn test_invest_validation() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let investor = Address::generate(&env);
    let pool_id = client.create_pool(&ids(&env), &vec![&env, 5_000u32, 3_000, 2_000]);

    assert_eq!(
        client.try_invest_pool(&investor, &pool_id, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        client.try_invest_pool(&investor, &99, &100),
        Err(Ok(Error::PoolNotFound))
    );
}

#[test]
#[should_panic]
fn test_create_pool_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(InvoicePool, ());
    let client = InvoicePoolClient::new(&env, &contract_id);
    client.initialize(&Address::generate(&env), &Address::generate(&env));
    client.create_pool(&ids(&env), &vec![&env, 5_000u32, 3_000, 2_000]);
}
