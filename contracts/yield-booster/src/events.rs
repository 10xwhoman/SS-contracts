use crate::types::Tier;
use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn yield_boost_applied(
    env: &Env,
    wallet: &Address,
    invoice_id: &BytesN<32>,
    tier: Tier,
    boost_bps: u32,
) {
    let topics = (
        Symbol::new(env, "YieldBoostApplied"),
        wallet.clone(),
        invoice_id.clone(),
    );
    env.events().publish(topics, (tier as u32, boost_bps));
}

pub fn boost_pool_funded(env: &Env, admin: &Address, amount: i128, new_balance: i128) {
    let topics = (Symbol::new(env, "BoostPoolFunded"), admin.clone());
    env.events().publish(topics, (amount, new_balance));
}
