#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env,
};

#[test]
fn test_initialize_and_default_score() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvestorScoring, ());
    let client = InvestorScoringClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &None);

    let info = client.get_score(&investor);
    assert_eq!(info.score, 100);
    assert_eq!(info.tier, Tier::Bronze);
}

#[test]
fn test_score_calculation_from_all_factors() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvestorScoring, ());
    let client = InvestorScoringClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &None);
    client.set_authorized_contract(&operator, &true);

    // Record activity: 10 investments (+150 pts), 0 defaults, 0 exits, 50_000_000 volume (+25 pts)
    // Base 100 + 150 + 25 = 275 (Bronze)
    let score1 = client.record_activity(&operator, &investor, &10, &0, &0, &50_000_000);
    assert_eq!(score1, 275);
    let info1 = client.get_score(&investor);
    assert_eq!(info1.tier, Tier::Bronze);

    // Now add 10 more investments (+150) -> total 425 (Silver threshold is 300)
    let score2 = client.record_activity(&operator, &investor, &10, &0, &0, &0);
    assert_eq!(score2, 425);
    let info2 = client.get_score(&investor);
    assert_eq!(info2.tier, Tier::Silver);

    // Now record a default penalty (-50 pts) -> 375
    let score3 = client.record_activity(&operator, &investor, &0, &1, &0, &0);
    assert_eq!(score3, 375);

    // Now record an early exit penalty (-20 pts) -> 355
    let score4 = client.record_activity(&operator, &investor, &0, &0, &1, &0);
    assert_eq!(score4, 355);
}

#[test]
fn test_tier_thresholds_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvestorScoring, ());
    let client = InvestorScoringClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let operator = Address::generate(&env);

    let custom_thresholds = TierThresholds {
        silver: 200,
        gold: 400,
        platinum: 700,
    };

    client.initialize(&admin, &Some(custom_thresholds));
    client.set_authorized_contract(&operator, &true);

    // Record activity: 10 investments -> score 250 (which is >= 200 silver)
    client.record_activity(&operator, &investor, &10, &0, &0, &0);

    let score = client.get_score(&investor);
    assert_eq!(score.tier, Tier::Silver);

    // Admin updates thresholds so silver is 300 -> drops back to bronze
    client.set_tier_thresholds(&TierThresholds {
        silver: 300,
        gold: 500,
        platinum: 800,
    });

    let score_after = client.get_score(&investor);
    assert_eq!(score_after.tier, Tier::Bronze);
}

#[test]
fn test_unauthorized_caller_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvestorScoring, ());
    let client = InvestorScoringClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let unauthorized_caller = Address::generate(&env);
    let investor = Address::generate(&env);

    client.initialize(&admin, &None);

    let res = client.try_record_activity(&unauthorized_caller, &investor, &5, &0, &0, &0);
    assert!(res.is_err());
}
