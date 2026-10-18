#![no_std]

mod events;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
pub use types::{AlertPreference, EventType, StorageKey};

#[contract]
pub struct AlertSubscription;

fn default_preferences(env: &Env) -> Vec<AlertPreference> {
    let mut prefs = Vec::new(env);
    for event_type in [
        EventType::Settlement,
        EventType::KycExpiry,
        EventType::WatchlistChange,
        EventType::AutoInvest,
        EventType::MaturityApproaching,
    ] {
        prefs.push_back(AlertPreference {
            event_type,
            enabled: true,
        });
    }
    prefs
}

fn load_preferences(env: &Env, wallet: &Address) -> Vec<AlertPreference> {
    env.storage()
        .persistent()
        .get(&StorageKey::Preferences(wallet.clone()))
        .unwrap_or_else(|| default_preferences(env))
}

#[contractimpl]
impl AlertSubscription {
    /// Wallet-only: update alert preferences. Event types that are not listed
    /// keep their current value. Emits `AlertPreferenceUpdated` with the
    /// preferences whose value actually changed.
    pub fn set_alert_preferences(env: Env, wallet: Address, preferences: Vec<AlertPreference>) {
        wallet.require_auth();

        let mut current = load_preferences(&env, &wallet);
        let mut changed = Vec::new(&env);

        for pref in preferences.iter() {
            for i in 0..current.len() {
                let existing = current.get(i).unwrap();
                if existing.event_type == pref.event_type {
                    if existing.enabled != pref.enabled {
                        current.set(i, pref.clone());
                        changed.push_back(pref.clone());
                    }
                    break;
                }
            }
        }

        env.storage()
            .persistent()
            .set(&StorageKey::Preferences(wallet.clone()), &current);

        if !changed.is_empty() {
            events::alert_preference_updated(&env, &wallet, &changed);
        }
    }

    /// View: all preferences for a wallet (all enabled by default).
    pub fn get_alert_preferences(env: Env, wallet: Address) -> Vec<AlertPreference> {
        load_preferences(&env, &wallet)
    }
}
