use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn vest_created(
    env: &Env,
    vest_id: u32,
    invoice_id: &BytesN<32>,
    beneficiary: &Address,
    total_amount: i128,
    tranche_count: u32,
) {
    let topics = (Symbol::new(env, "VestCreated"), vest_id);
    env.events().publish(
        topics,
        (
            invoice_id.clone(),
            beneficiary.clone(),
            total_amount,
            tranche_count,
        ),
    );
}

pub fn tranche_released(
    env: &Env,
    vest_id: u32,
    tranche_index: u32,
    beneficiary: &Address,
    amount: i128,
) {
    let topics = (Symbol::new(env, "TrancheReleased"), vest_id, tranche_index);
    let timestamp = env.ledger().timestamp();
    env.events().publish(topics, (beneficiary.clone(), amount, timestamp));
}
