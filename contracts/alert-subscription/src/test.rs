#![allow(deprecated)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{vec, Address, Env, Symbol, TryFromVal, Val, Vec};

fn pref(event_type: EventType, enabled: bool) -> AlertPreference {
    AlertPreference {
        event_type,
        enabled,
    }
}

/// Count recorded events whose first topic equals `name`.
fn count_events(env: &Env, name: &str) -> u32 {
    let target = Symbol::new(env, name);
    let mut count = 0;
    for event in env.events().all().events().iter() {
        let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
        let topics = Vec::<Val>::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
        )
        .unwrap();
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(env, &first) {
                if sym == target {
                    count += 1;
                }
            }
        }
    }
    count
}

#[test]
fn new_wallet_defaults_to_all_enabled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = AlertSubscriptionClient::new(&env, &env.register(AlertSubscription, ()));
    let wallet = Address::generate(&env);

    let prefs = client.get_alert_preferences(&wallet);
    assert_eq!(prefs.len(), 5);
    for p in prefs.iter() {
        assert!(p.enabled);
    }
}

#[test]
fn set_preferences_updates_only_listed_types_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = AlertSubscriptionClient::new(&env, &env.register(AlertSubscription, ()));
    let wallet = Address::generate(&env);

    client.set_alert_preferences(
        &wallet,
        &vec![
            &env,
            pref(EventType::KycExpiry, false),
            pref(EventType::AutoInvest, false),
        ],
    );
    assert_eq!(count_events(&env, "AlertPreferenceUpdated"), 1);

    for p in client.get_alert_preferences(&wallet).iter() {
        let expected = !matches!(p.event_type, EventType::KycExpiry | EventType::AutoInvest);
        assert_eq!(p.enabled, expected);
    }

    // Preferences are stored per wallet.
    let other = Address::generate(&env);
    for p in client.get_alert_preferences(&other).iter() {
        assert!(p.enabled);
    }

    // Re-enable one type.
    client.set_alert_preferences(&wallet, &vec![&env, pref(EventType::KycExpiry, true)]);
    for p in client.get_alert_preferences(&wallet).iter() {
        assert_eq!(p.enabled, p.event_type != EventType::AutoInvest);
    }
}

#[test]
fn unchanged_preferences_emit_no_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = AlertSubscriptionClient::new(&env, &env.register(AlertSubscription, ()));
    let wallet = Address::generate(&env);

    client.set_alert_preferences(&wallet, &vec![&env, pref(EventType::Settlement, true)]);
    assert_eq!(count_events(&env, "AlertPreferenceUpdated"), 0);
}

#[test]
fn set_preferences_requires_wallet_auth() {
    let env = Env::default();
    let client = AlertSubscriptionClient::new(&env, &env.register(AlertSubscription, ()));
    let wallet = Address::generate(&env);

    let result = client
        .try_set_alert_preferences(&wallet, &vec![&env, pref(EventType::Settlement, false)]);
    assert!(result.is_err());
}
