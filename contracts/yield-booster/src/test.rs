#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    Address, BytesN, Env,
};

#[contracttype]
enum MockKey {
    Tier(Address),
}

#[contract]
struct MockLoyalty;

#[contractimpl]
impl MockLoyalty {
    pub fn set_tier(env: Env, wallet: Address, tier: Tier) {
        env.storage()
            .persistent()
            .set(&MockKey::Tier(wallet), &tier);
    }

    pub fn get_loyalty_tier(env: Env, wallet: Address) -> LoyaltyInfo {
        let tier = env
            .storage()
            .persistent()
            .get(&MockKey::Tier(wallet))
            .unwrap_or(Tier::Bronze);
        LoyaltyInfo {
            tier,
            cumulative_volume: 0,
            next_threshold: None,
        }
    }
}

struct Ctx<'a> {
    booster: YieldBoosterClient<'a>,
    loyalty: MockLoyaltyClient<'a>,
    token: Address,
    admin: Address,
}

fn setup(env: &Env) -> Ctx<'_> {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let settlement = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let loyalty_id = env.register(MockLoyalty, ());
    let booster_id = env.register(YieldBooster, ());
    let booster = YieldBoosterClient::new(env, &booster_id);
    booster.initialize(&admin, &settlement, &loyalty_id, &token);

    Ctx {
        loyalty: MockLoyaltyClient::new(env, &loyalty_id),
        booster,
        token,
        admin,
    }
}

#[test]
fn test_boost_for_each_tier() {
    let env = Env::default();
    let c = setup(&env);
    let wallet = Address::generate(&env);
    let invoice = BytesN::from_array(&env, &[1u8; 32]);

    assert_eq!(c.booster.get_boost_amount(&wallet, &invoice), 0);

    c.loyalty.set_tier(&wallet, &Tier::Silver);
    assert_eq!(c.booster.get_boost_amount(&wallet, &invoice), 25);
    c.loyalty.set_tier(&wallet, &Tier::Gold);
    assert_eq!(c.booster.get_boost_amount(&wallet, &invoice), 50);
    c.loyalty.set_tier(&wallet, &Tier::Platinum);
    assert_eq!(c.booster.apply_boost(&wallet, &invoice), 100);
    assert_eq!(c.booster.get_boost_amount(&wallet, &invoice), 100);
}

#[test]
fn test_no_boost_below_silver() {
    let env = Env::default();
    let c = setup(&env);
    let wallet = Address::generate(&env);
    let invoice = BytesN::from_array(&env, &[2u8; 32]);

    assert_eq!(c.booster.apply_boost(&wallet, &invoice), 0);
    assert_eq!(env.events().all().events().len(), 0);
}

#[test]
fn test_apply_boost_emits_event_and_is_applied_once() {
    let env = Env::default();
    let c = setup(&env);
    let wallet = Address::generate(&env);
    let invoice = BytesN::from_array(&env, &[3u8; 32]);

    c.loyalty.set_tier(&wallet, &Tier::Gold);
    assert_eq!(c.booster.apply_boost(&wallet, &invoice), 50);
    assert_eq!(env.events().all().events().len(), 1);

    assert_eq!(
        c.booster.try_apply_boost(&wallet, &invoice),
        Err(Ok(Error::BoostAlreadyApplied))
    );
}

#[test]
fn test_fund_boost_pool() {
    let env = Env::default();
    let c = setup(&env);
    AssetClient::new(&env, &c.token).mint(&c.admin, &1_000);

    assert_eq!(c.booster.fund_boost_pool(&600), 600);
    assert_eq!(c.booster.fund_boost_pool(&400), 1_000);
    assert_eq!(c.booster.get_boost_pool(), 1_000);
    assert_eq!(
        TokenClient::new(&env, &c.token).balance(&c.booster.address),
        1_000
    );
    assert_eq!(
        c.booster.try_fund_boost_pool(&0),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
#[should_panic]
fn test_fund_boost_pool_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let booster_id = env.register(YieldBooster, ());
    let booster = YieldBoosterClient::new(&env, &booster_id);
    booster.initialize(
        &admin,
        &Address::generate(&env),
        &Address::generate(&env),
        &token,
    );
    booster.fund_boost_pool(&100);
}
