#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env,
};

fn setup(env: &Env) -> (LoyaltyTierClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(LoyaltyTier, ());
    let client = LoyaltyTierClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let escrow = Address::generate(env);
    client.initialize(&admin, &escrow);
    (client, escrow)
}

#[test]
fn test_tier_assigned_at_each_volume_level() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let wallet = Address::generate(&env);

    let info = client.get_loyalty_tier(&wallet);
    assert_eq!(info.tier, Tier::Bronze);
    assert_eq!(info.cumulative_volume, 0);
    assert_eq!(info.next_threshold, Some(10_000));

    assert_eq!(client.update_loyalty(&wallet, &9_999), Tier::Bronze);
    assert_eq!(client.update_loyalty(&wallet, &1), Tier::Silver);
    assert_eq!(client.update_loyalty(&wallet, &90_000), Tier::Gold);
    assert_eq!(client.update_loyalty(&wallet, &400_000), Tier::Platinum);

    let info = client.get_loyalty_tier(&wallet);
    assert_eq!(info.tier, Tier::Platinum);
    assert_eq!(info.cumulative_volume, 500_000);
    assert_eq!(info.next_threshold, None);
}

#[test]
fn test_progress_to_next_threshold() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let wallet = Address::generate(&env);

    client.update_loyalty(&wallet, &10_000);
    let info = client.get_loyalty_tier(&wallet);
    assert_eq!(info.tier, Tier::Silver);
    assert_eq!(info.next_threshold, Some(100_000));
}

#[test]
fn test_upgrade_event_only_on_boundary_cross() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let wallet = Address::generate(&env);

    client.update_loyalty(&wallet, &5_000);
    assert_eq!(env.events().all().events().len(), 0);

    client.update_loyalty(&wallet, &5_000);
    assert_eq!(env.events().all().events().len(), 1);
}

#[test]
fn test_set_thresholds_and_validation() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let wallet = Address::generate(&env);

    let custom = LoyaltyThresholds {
        silver: 100,
        gold: 200,
        platinum: 300,
    };
    client.set_loyalty_thresholds(&custom);
    assert_eq!(client.get_loyalty_thresholds(), custom);
    assert_eq!(client.update_loyalty(&wallet, &250), Tier::Gold);

    let bad = LoyaltyThresholds {
        silver: 300,
        gold: 200,
        platinum: 100,
    };
    assert_eq!(
        client.try_set_loyalty_thresholds(&bad),
        Err(Ok(Error::InvalidThresholds))
    );
}

#[test]
fn test_invalid_amount_rejected() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let wallet = Address::generate(&env);
    assert_eq!(
        client.try_update_loyalty(&wallet, &0),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
#[should_panic]
fn test_update_loyalty_requires_escrow_auth() {
    let env = Env::default();
    let contract_id = env.register(LoyaltyTier, ());
    let client = LoyaltyTierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let escrow = Address::generate(&env);
    client.initialize(&admin, &escrow);

    let wallet = Address::generate(&env);
    client.update_loyalty(&wallet, &100);
}
