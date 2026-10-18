use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    vec, Address, Env, U256,
};

fn setup(env: &Env) -> NonceRegistryClient<'_> {
    let contract_id = env.register(NonceRegistry, ());
    let client = NonceRegistryClient::new(env, &contract_id);
    client.initialize(&Address::generate(env));
    client
}

#[test]
fn test_consume_nonce_rejects_reuse() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let wallet = Address::generate(&env);
    let nonce = U256::from_u32(&env, 7);

    assert!(!client.is_nonce_used(&wallet, &nonce));
    client.consume_nonce(&wallet, &nonce);
    assert!(client.is_nonce_used(&wallet, &nonce));
    assert_eq!(env.events().all().events().len(), 1);

    let res = client.try_consume_nonce(&wallet, &nonce);
    assert_eq!(res, Err(Ok(Error::NonceAlreadyUsed)));

    // Scoped per wallet, and no ordering: a lower nonce is still usable.
    assert!(!client.is_nonce_used(&Address::generate(&env), &nonce));
    client.consume_nonce(&wallet, &U256::from_u32(&env, 3));
}

#[test]
fn test_bulk_invalidate_burns_all_nonces() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let wallet = Address::generate(&env);
    let used = U256::from_u32(&env, 1);
    client.consume_nonce(&wallet, &used);

    let nonces = vec![&env, used.clone(), U256::from_u32(&env, 2), U256::from_u32(&env, 3)];
    client.bulk_invalidate(&wallet, &nonces);

    assert_eq!(env.auths()[0].0, wallet);
    for nonce in nonces.iter() {
        assert!(client.is_nonce_used(&wallet, &nonce));
    }
    let res = client.try_consume_nonce(&wallet, &U256::from_u32(&env, 2));
    assert_eq!(res, Err(Ok(Error::NonceAlreadyUsed)));
}

#[test]
fn test_bulk_invalidate_requires_wallet_auth() {
    let env = Env::default();
    let client = setup(&env);
    let wallet = Address::generate(&env);

    let res = client.try_bulk_invalidate(&wallet, &vec![&env, U256::from_u32(&env, 1)]);
    assert!(res.is_err());
    assert!(!client.is_nonce_used(&wallet, &U256::from_u32(&env, 1)));
}

#[test]
fn test_bulk_invalidate_rejects_oversized_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let mut nonces = Vec::new(&env);
    for i in 0..=MAX_BULK_INVALIDATE {
        nonces.push_back(U256::from_u32(&env, i));
    }

    let res = client.try_bulk_invalidate(&Address::generate(&env), &nonces);
    assert_eq!(res, Err(Ok(Error::BatchTooLarge)));
}
