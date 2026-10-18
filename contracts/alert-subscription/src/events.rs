use crate::types::AlertPreference;
use soroban_sdk::{Address, Env, Symbol, Vec};

pub fn alert_preference_updated(env: &Env, wallet: &Address, changed: &Vec<AlertPreference>) {
    let topics = (Symbol::new(env, "AlertPreferenceUpdated"), wallet.clone());
    env.events().publish(topics, changed.clone());
}
