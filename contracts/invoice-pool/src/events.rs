use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

pub fn pool_created(env: &Env, pool_id: u32, invoice_ids: &Vec<BytesN<32>>, weights: &Vec<u32>) {
    let topics = (Symbol::new(env, "PoolCreated"), pool_id);
    env.events()
        .publish(topics, (invoice_ids.clone(), weights.clone()));
}

pub fn pool_investment_executed(
    env: &Env,
    pool_id: u32,
    investor: &Address,
    amount: i128,
    allocations: &Vec<i128>,
) {
    let topics = (
        Symbol::new(env, "PoolInvestmentExecuted"),
        pool_id,
        investor.clone(),
    );
    env.events().publish(topics, (amount, allocations.clone()));
}
