#![allow(deprecated, clippy::all)]

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use super::*;

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

// ── Registration tests ──────────────────────────────────────────────────────

#[test]
fn test_register_referral_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);

    // Referral registration event emitted (captured before any view call,
    // since a new invocation replaces the recorded event set).
    let events = env.events().all();
    let evt = events
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_, topics, _) = parse_event(&env, e);
            topics
                .get(0)
                .map(|t| {
                    Symbol::try_from_val(&env, &t)
                        .map(|s| s == Symbol::new(&env, "referral_registered"))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .expect("expected referral_registered event");
    let (_, _, data) = parse_event(&env, evt);
    let (r, ref_addr): (Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(r, referee);
    assert_eq!(ref_addr, referrer);

    // Verify the referrer is stored.
    let stored = referral.get_referrer(&referee);
    assert_eq!(stored, Some(referrer));
}

#[test]
fn test_register_referral_already_referred() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);
    let other_referrer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);

    // Second registration must fail.
    let result = referral.try_register_referral(&referee, &other_referrer);
    assert_eq!(result, Err(Ok(crate::errors::Error::AlreadyReferred)));

    // Original referrer is unchanged.
    let stored = referral.get_referrer(&referee);
    assert_eq!(stored, Some(referrer));
}

#[test]
fn test_register_self_referral_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    let result = referral.try_register_referral(&wallet, &wallet);
    assert_eq!(result, Err(Ok(crate::errors::Error::SelfReferral)));
}

// ── Fee accrual tests ───────────────────────────────────────────────────────

#[test]
fn test_accrue_referral_fee_correct_percentage() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address()); // 5%

    referral.register_referral(&referee, &referrer);

    let inv_id = Symbol::new(&env, "INV001");
    let amount = 10_000i128;
    referral.accrue_referral_fee(&Address::generate(&env), &inv_id, &referee, &amount);

    // 5% of 10_000 = 500
    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 500);
    assert_eq!(stats.pending, 500);
    assert_eq!(stats.referee_count, 1);
}

#[test]
fn test_accrue_no_referrer_is_noop() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    // No referrer registered — should be a no-op (Ok).
    let inv_id = Symbol::new(&env, "INV001");
    let result =
        referral.try_accrue_referral_fee(&Address::generate(&env), &inv_id, &referee, &10_000);
    assert!(result.is_ok());

    // No stats should exist for this referee's (nonexistent) referrer.
}

#[test]
fn test_referral_fee_paid_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);

    let inv_id = Symbol::new(&env, "INV001");
    referral.accrue_referral_fee(&Address::generate(&env), &inv_id, &referee, &10_000);

    let events = env.events().all();
    let evt = events
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_, topics, _) = parse_event(&env, e);
            topics
                .get(0)
                .map(|t| {
                    Symbol::try_from_val(&env, &t)
                        .map(|s| s == Symbol::new(&env, "referral_fee_paid"))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .expect("expected referral_fee_paid event");
    let (_, _, data) = parse_event(&env, evt);
    let (id, r, ref_addr, fee): (Symbol, Address, Address, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(id, inv_id);
    assert_eq!(r, referee);
    assert_eq!(ref_addr, referrer);
    assert_eq!(fee, 500);
}

#[test]
fn test_multiple_accruals_accumulate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);

    // Two investments of 10_000 each → 500 + 500 = 1000 total
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV1"),
        &referee,
        &10_000,
    );
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV2"),
        &referee,
        &10_000,
    );

    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 1000);
    assert_eq!(stats.pending, 1000);
}

// ── Claim rewards tests ─────────────────────────────────────────────────────

#[test]
fn test_claim_rewards_transfers_correct_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = TokenClient::new(&env, &token_id.address());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV1"),
        &referee,
        &10_000,
    );

    // Pending = 500. Claim should transfer 500 to referrer.
    let balance_before = token.balance(&referrer);
    referral.claim_rewards(&referrer);
    assert_eq!(token.balance(&referrer), balance_before + 500);

    // Pending should be zero after claim.
    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.total_earned, 500); // total_earned unchanged
}

#[test]
fn test_claim_rewards_nothing_to_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    let result = referral.try_claim_rewards(&referrer);
    assert_eq!(result, Err(Ok(crate::errors::Error::NothingToClaim)));
}

// ── get_referral_stats tests ─────────────────────────────────────────────────

#[test]
fn test_get_referral_stats_accurate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee1 = Address::generate(&env);
    let referee2 = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &1000, &token_id.address()); // 10%

    // Register two referees.
    referral.register_referral(&referee1, &referrer);
    referral.register_referral(&referee2, &referrer);

    // Accrue fees for both.
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV1"),
        &referee1,
        &10_000,
    ); // 1000
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV2"),
        &referee2,
        &5_000,
    ); // 500

    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 1500);
    assert_eq!(stats.pending, 1500);
    assert_eq!(stats.referee_count, 2);

    // Claim and check pending resets.
    referral.claim_rewards(&referrer);
    let stats2 = referral.get_referral_stats(&referrer);
    assert_eq!(stats2.total_earned, 1500);
    assert_eq!(stats2.pending, 0);
    assert_eq!(stats2.referee_count, 2);
}

// ── Admin function tests ────────────────────────────────────────────────────

#[test]
fn test_set_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    referral.register_referral(&referee, &referrer);

    // Initial fee = 5% → 500 of 10_000
    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV1"),
        &referee,
        &10_000,
    );
    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.pending, 500);

    // Update fee to 10% → 1000 of 10_000
    referral.set_fee_bps(&admin, &1000);
    assert_eq!(referral.get_config().fee_bps, 1000);

    referral.accrue_referral_fee(
        &Address::generate(&env),
        &Symbol::new(&env, "INV2"),
        &referee,
        &10_000,
    );
    let stats2 = referral.get_referral_stats(&referrer);
    assert_eq!(stats2.pending, 500 + 1000);
}

#[test]
fn test_set_fee_bps_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let impostor = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    let result = referral.try_set_fee_bps(&impostor, &1000);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    let result = referral.try_initialize(&admin, &1000, &token_id.address());
    assert_eq!(result, Err(Ok(crate::errors::Error::AlreadyInit)));
}

#[test]
fn test_initialize_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let asset = AssetClient::new(&env, &token_id.address());

    let referral_id = env.register(Referral, ());
    asset.mint(&referral_id, &100_000);
    let referral = ReferralClient::new(&env, &referral_id);

    let result = referral.try_initialize(&admin, &10_001, &token_id.address());
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidFeeBps)));
}

// ── Escrow caller binding tests ─────────────────────────────────────────────

#[test]
fn test_accrue_open_when_no_escrow_bound() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());
    referral.register_referral(&referee, &referrer);

    // No escrow bound: any authorized caller may accrue.
    let caller = Address::generate(&env);
    referral.accrue_referral_fee(&caller, &Symbol::new(&env, "INV1"), &referee, &10_000);

    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.pending, 500);
}

#[test]
fn test_accrue_bound_escrow_only() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());
    referral.register_referral(&referee, &referrer);

    let bound_escrow = Address::generate(&env);
    referral.set_escrow(&admin, &Some(bound_escrow.clone()));

    // A different caller is rejected.
    let other = Address::generate(&env);
    let result =
        referral.try_accrue_referral_fee(&other, &Symbol::new(&env, "INV1"), &referee, &10_000);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));

    // Nothing accrued for the rejected call.
    assert_eq!(referral.get_referral_stats(&referrer).pending, 0);

    // The bound escrow succeeds.
    referral.accrue_referral_fee(&bound_escrow, &Symbol::new(&env, "INV1"), &referee, &10_000);
    assert_eq!(referral.get_referral_stats(&referrer).pending, 500);
}

#[test]
fn test_set_escrow_none_clears_binding() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);
    let escrow = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());
    referral.register_referral(&referee, &referrer);

    referral.set_escrow(&admin, &Some(escrow.clone()));
    assert_eq!(referral.get_config().escrow, Some(escrow.clone()));

    referral.set_escrow(&admin, &None);
    assert_eq!(referral.get_config().escrow, None);

    // After clearing, any authorized caller is accepted again.
    let caller = Address::generate(&env);
    referral.accrue_referral_fee(&caller, &Symbol::new(&env, "INV1"), &referee, &10_000);
    assert_eq!(referral.get_referral_stats(&referrer).pending, 500);
}

#[test]
fn test_set_escrow_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());

    let attacker = Address::generate(&env);
    let escrow = Address::generate(&env);
    let result = referral.try_set_escrow(&attacker, &Some(escrow));
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
}

#[test]
fn test_accrue_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let referrer = Address::generate(&env);
    let referee = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&admin, &500, &token_id.address());
    referral.register_referral(&referee, &referrer);

    let escrow = Address::generate(&env);
    let result =
        referral.try_accrue_referral_fee(&escrow, &Symbol::new(&env, "INV1"), &referee, &0);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));

    let result =
        referral.try_accrue_referral_fee(&escrow, &Symbol::new(&env, "INV2"), &referee, &-100);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
}
