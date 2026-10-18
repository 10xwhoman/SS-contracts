use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env,
};

fn setup(env: &Env) -> (OnboardingBonusClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(OnboardingBonus, ());
    let client = OnboardingBonusClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    client.initialize(&admin, &token, &Address::generate(env));
    StellarAssetClient::new(env, &token).mint(&admin, &10_000);
    (client, admin, token)
}

fn invoice(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn test_first_investment_receives_half_percent_bonus() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    client.fund_bonus_pool(&1_000);
    assert_eq!(client.get_bonus_pool(), 1_000);

    let wallet = Address::generate(&env);
    assert!(!client.get_bonus_status(&wallet).claimed);

    let bonus = client.claim_onboarding_bonus(&wallet, &invoice(&env, 1), &10_000);
    assert_eq!(bonus, 50);
    assert_eq!(TokenClient::new(&env, &token).balance(&wallet), 50);
    assert_eq!(client.get_bonus_pool(), 950);

    let status = client.get_bonus_status(&wallet);
    assert!(status.claimed);
    assert_eq!(status.bonus_amount, 50);
    assert_eq!(status.invoice_id, Some(invoice(&env, 1)));
}

#[test]
fn test_second_claim_rejected_with_already_claimed() {
    let env = Env::default();
    let (client, _admin, _token) = setup(&env);
    client.fund_bonus_pool(&1_000);
    let wallet = Address::generate(&env);
    client.claim_onboarding_bonus(&wallet, &invoice(&env, 1), &10_000);

    let res = client.try_claim_onboarding_bonus(&wallet, &invoice(&env, 2), &10_000);
    assert_eq!(res, Err(Ok(Error::AlreadyClaimed)));
}

#[test]
fn test_fund_bonus_pool_requires_admin_and_pool_must_cover_bonus() {
    let env = Env::default();
    let contract_id = env.register(OnboardingBonus, ());
    let client = OnboardingBonusClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    client.initialize(&admin, &token, &Address::generate(&env));

    // No auth mocked: the admin check must reject the call.
    assert!(client.try_fund_bonus_pool(&100).is_err());

    env.mock_all_auths();
    StellarAssetClient::new(&env, &token).mint(&admin, &100);
    client.fund_bonus_pool(&10);
    assert_eq!(env.auths()[0].0, admin);

    let wallet = Address::generate(&env);
    let res = client.try_claim_onboarding_bonus(&wallet, &invoice(&env, 1), &10_000);
    assert_eq!(res, Err(Ok(Error::InsufficientBonusPool)));
    assert!(!client.get_bonus_status(&wallet).claimed);
}
