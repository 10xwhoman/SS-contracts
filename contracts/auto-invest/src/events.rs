use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn auto_invest_executed(
    env: &Env,
    wallet: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
) {
    let topics = (Symbol::new(env, "AutoInvestExecuted"), wallet.clone());
    let timestamp = env.ledger().timestamp();
    env.events()
        .publish(topics, (invoice_id.clone(), amount, timestamp));
}

pub fn auto_invest_rule_updated(
    env: &Env,
    wallet: &Address,
    max_amount: i128,
    budget: i128,
    active: bool,
) {
    let topics = (Symbol::new(env, "AutoInvestRuleUpdated"), wallet.clone());
    env.events().publish(topics, (max_amount, budget, active));
}

pub fn auto_invest_disabled(env: &Env, wallet: &Address) {
    let topics = (Symbol::new(env, "AutoInvestDisabled"), wallet.clone());
    env.events().publish(topics, ());
}
