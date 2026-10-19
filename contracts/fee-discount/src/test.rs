#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env,
};

fn setup(env: &Env) -> (FeeDiscountClient<'_>, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let loyalty = Address::generate(env);
    let contract_id = env.register(FeeDiscount, ());
    let client = FeeDiscountClient::new(env, &contract_id);
    // 200 bps = 2% platform fee
    client.initialize(&admin, &loyalty, &200u32);
    (client, admin, loyalty)
}

#[test]
fn bronze_receives_no_discount() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    // Bronze (default) → 0 bps discount
    assert_eq!(client.get_fee_discount(&wallet), 0_i128);
}

#[test]
fn silver_receives_five_percent_discount() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Silver);
    assert_eq!(client.get_fee_discount(&wallet), 500_i128);
}

#[test]
fn gold_receives_ten_percent_discount() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Gold);
    assert_eq!(client.get_fee_discount(&wallet), 1_000_i128);
}

#[test]
fn platinum_receives_twenty_percent_discount() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Platinum);
    assert_eq!(client.get_fee_discount(&wallet), 2_000_i128);
}

#[test]
fn invest_bronze_pays_full_fee() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    // 10_000 investment * 2% = 200 fee, 0% discount → 200 final fee
    assert_eq!(client.invest(&wallet, &10_000_i128), 200_i128);
}

#[test]
fn invest_silver_discounts_five_percent() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Silver);
    // original_fee = 10_000 * 200 / 10_000 = 200
    // discount = 200 * 500 / 10_000 = 10
    // final_fee = 200 - 10 = 190
    assert_eq!(client.invest(&wallet, &10_000_i128), 190_i128);
}

#[test]
fn invest_gold_discounts_ten_percent() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Gold);
    // final_fee = 200 - (200 * 1000/10000) = 200 - 20 = 180
    assert_eq!(client.invest(&wallet, &10_000_i128), 180_i128);
}

#[test]
fn invest_platinum_discounts_twenty_percent() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Platinum);
    // final_fee = 200 - (200 * 2000/10000) = 200 - 40 = 160
    assert_eq!(client.invest(&wallet, &10_000_i128), 160_i128);
}

#[test]
fn buy_applies_discount_correctly() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Gold);
    // same logic as invest for Gold
    assert_eq!(client.buy(&wallet, &10_000_i128), 180_i128);
}

#[test]
fn invest_zero_amount_rejected() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    assert_eq!(
        client.try_invest(&wallet, &0_i128),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn fee_discount_applied_event_emitted() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let wallet = Address::generate(&env);
    client.set_wallet_tier(&wallet, &LoyaltyTier::Silver);
    client.invest(&wallet, &10_000_i128);
    // At least one event should be published
    assert!(env.events().all().events().len() >= 1);
}

#[test]
fn unregistered_wallet_gets_bronze_zero_discount() {
    let env = Env::default();
    let (client, _admin, _loyalty) = setup(&env);
    let unregistered = Address::generate(&env);
    assert_eq!(client.get_fee_discount(&unregistered), 0_i128);
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let (client, admin, loyalty) = setup(&env);
    assert_eq!(
        client.try_initialize(&admin, &loyalty, &200u32),
        Err(Ok(Error::AlreadyInitialized))
    );
}
