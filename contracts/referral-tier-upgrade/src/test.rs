#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env,
};

fn setup(env: &Env) -> (ReferralTierUpgradeClient<'_>, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let referral_contract = Address::generate(env);
    let contract_id = env.register(ReferralTierUpgrade, ());
    let client = ReferralTierUpgradeClient::new(env, &contract_id);
    client.initialize(&admin, &referral_contract);
    (client, admin, referral_contract)
}

#[test]
fn new_wallet_starts_at_standard() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    let info = client.get_referral_tier(&wallet);
    assert_eq!(info.tier, ReferralTier::Standard);
    assert_eq!(info.cumulative_volume, 0);
    assert_eq!(info.next_tier_threshold, Some(50_000_i128));
}

#[test]
fn commission_bps_standard_is_2000() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    assert_eq!(client.get_commission_bps(&wallet), 2_000_i128);
}

#[test]
fn volume_below_threshold_stays_standard() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &49_999_i128);
    let info = client.get_referral_tier(&wallet);
    assert_eq!(info.tier, ReferralTier::Standard);
}

#[test]
fn crossing_50k_upgrades_to_plus() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &50_000_i128);
    let info = client.get_referral_tier(&wallet);
    assert_eq!(info.tier, ReferralTier::Plus);
    assert_eq!(info.next_tier_threshold, Some(500_000_i128));
    assert_eq!(client.get_commission_bps(&wallet), 2_500_i128);
}

#[test]
fn crossing_500k_upgrades_to_elite() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &500_000_i128);
    let info = client.get_referral_tier(&wallet);
    assert_eq!(info.tier, ReferralTier::Elite);
    assert_eq!(info.next_tier_threshold, None);
    assert_eq!(client.get_commission_bps(&wallet), 3_000_i128);
}

#[test]
fn tier_upgrade_event_emitted_when_boundary_crossed() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &50_000_i128);
    assert_eq!(env.events().all().events().len(), 1);
}

#[test]
fn no_event_emitted_when_tier_unchanged() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &1_000_i128);
    // No tier boundary crossed → no ReferralTierUpgraded event
    assert_eq!(env.events().all().events().len(), 0);
}

#[test]
fn update_referral_tier_recomputes_correctly() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &75_000_i128);
    let tier = client.update_referral_tier(&wallet);
    assert_eq!(tier, ReferralTier::Plus);
}

#[test]
fn zero_volume_rejected() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    assert_eq!(
        client.try_on_referral_reward_paid(&wallet, &0_i128),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let (client, admin, ref_contract) = setup(&env);
    assert_eq!(
        client.try_initialize(&admin, &ref_contract),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn cumulative_volume_accumulates_across_calls() {
    let env = Env::default();
    let (client, _admin, _ref_contract) = setup(&env);
    let wallet = Address::generate(&env);
    client.on_referral_reward_paid(&wallet, &30_000_i128);
    client.on_referral_reward_paid(&wallet, &25_000_i128);
    let info = client.get_referral_tier(&wallet);
    assert_eq!(info.cumulative_volume, 55_000_i128);
    assert_eq!(info.tier, ReferralTier::Plus);
}
