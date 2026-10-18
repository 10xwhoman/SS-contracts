use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    Address, BytesN, Env,
};

fn setup<'a>(
    env: &'a Env,
) -> (
    InvoiceInsuranceClient<'a>,
    Address,
    TokenClient<'a>,
    AssetClient<'a>,
) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceInsurance, ());
    let client = InvoiceInsuranceClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = TokenClient::new(env, &token_id.address());
    let asset = AssetClient::new(env, &token_id.address());
    client.initialize(&admin, &token_id.address());
    (client, admin, token, asset)
}

#[test]
fn premium_is_two_percent_of_coverage_amount() {
    let env = Env::default();
    let (client, _admin, token, asset) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);

    // Coverage amount = 10_000 * 50% = 5_000; premium = 2% of 5_000 = 100.
    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);

    assert_eq!(token.balance(&investor), 900);
    assert_eq!(token.balance(&client.address), 100);
    let policy = client.get_policy(&invoice_id, &investor).unwrap();
    assert_eq!(policy.coverage_amount, 5_000);
    assert_eq!(policy.premium, 100);
}

#[test]
fn rejects_invalid_and_duplicate_policies() {
    let env = Env::default();
    let (client, _admin, _token, asset) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);

    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &0),
        Err(Ok(Error::InvalidCoverage))
    );
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &10_001),
        Err(Ok(Error::InvalidCoverage))
    );
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &0, &5_000),
        Err(Ok(Error::InvalidAmount))
    );

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &5_000),
        Err(Ok(Error::PolicyAlreadyExists))
    );
}

#[test]
fn payout_covers_coverage_bps_of_principal_loss() {
    let env = Env::default();
    let (client, admin, token, asset) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    asset.mint(&client.address, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);

    // 40% principal loss = 4_000; 50% of it is covered = 2_000.
    let paid = client.trigger_insurance_payout(&invoice_id, &4_000);
    assert_eq!(env.auths()[0].0, admin);
    assert_eq!(paid, 2_000);
    assert_eq!(token.balance(&investor), 900 + 2_000);

    assert_eq!(
        client.try_trigger_insurance_payout(&invoice_id, &4_000),
        Err(Ok(Error::PayoutAlreadyTriggered))
    );
}

#[test]
fn payout_is_capped_at_coverage_amount() {
    let env = Env::default();
    let (client, _admin, token, asset) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    asset.mint(&client.address, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &10_000);

    // Total loss with full coverage pays exactly the coverage amount.
    let paid = client.trigger_insurance_payout(&invoice_id, &10_000);
    assert_eq!(paid, 10_000);
    assert_eq!(token.balance(&investor), 1_000 - 200 + 10_000);
}

#[test]
fn payout_fails_when_pool_is_insufficient() {
    let env = Env::default();
    let (client, _admin, _token, asset) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);
    assert_eq!(
        client.try_trigger_insurance_payout(&invoice_id, &10_000),
        Err(Ok(Error::InsufficientPool))
    );
}
