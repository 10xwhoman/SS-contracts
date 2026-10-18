use soroban_sdk::{BytesN, Env, Symbol};

pub fn partial_settlement_executed(
    env: &Env,
    invoice_id: &BytesN<32>,
    amount: i128,
    cumulative_paid: i128,
    remaining_balance: i128,
    is_settled: bool,
) {
    let topics = (Symbol::new(env, "PartialSettlementExecuted"), invoice_id.clone());
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            amount,
            cumulative_paid,
            remaining_balance,
            is_settled,
            timestamp,
        ),
    );
}
