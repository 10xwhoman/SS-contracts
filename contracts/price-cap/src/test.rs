use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env,
};

fn setup(env: &Env) -> PriceCapClient<'_> {
    let contract_id = env.register(PriceCap, ());
    let client = PriceCapClient::new(env, &contract_id);
    client.initialize(&Address::generate(env), &15_000);
    client
}

fn invoice(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

#[test]
fn test_cap_computed_from_yield_and_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    // 10% yield * 1.5x = 15% uplift on a price of 1_000.
    client.record_settlement(&invoice(&env), &1_000, &1_000);
    assert_eq!(client.get_price_cap(&invoice(&env)), 1_150);
}

#[test]
fn test_listing_blocked_above_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    client.record_settlement(&invoice(&env), &1_000, &1_000);

    client.validate_listing_price(&invoice(&env), &1_150);
    let res = client.try_validate_listing_price(&invoice(&env), &1_151);
    assert_eq!(res, Err(Ok(Error::PriceCapExceeded)));

    let unknown = BytesN::from_array(&env, &[2u8; 32]);
    let res = client.try_get_price_cap(&unknown);
    assert_eq!(res, Err(Ok(Error::InvoiceNotRecorded)));
}

#[test]
fn test_multiplier_update_is_admin_only_and_emits_event() {
    let env = Env::default();
    let contract_id = env.register(PriceCap, ());
    let client = PriceCapClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &15_000);

    // No auth mocked: the admin check must reject the call.
    assert!(client.try_set_price_cap_multiplier(&20_000).is_err());

    env.mock_all_auths();
    client.set_price_cap_multiplier(&20_000);
    assert_eq!(env.auths()[0].0, admin);
    assert_eq!(client.get_price_cap_multiplier(), 20_000);
    assert_eq!(env.events().all().events().len(), 1);

    let res = client.try_set_price_cap_multiplier(&0);
    assert_eq!(res, Err(Ok(Error::InvalidMultiplier)));
}
