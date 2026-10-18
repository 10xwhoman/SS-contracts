use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn buy_order_placed(
    env: &Env,
    order_id: u32,
    buyer: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    max_price: i128,
) {
    let topics = (Symbol::new(env, "BuyOrderPlaced"), order_id);
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            buyer.clone(),
            invoice_id.clone(),
            amount,
            max_price,
            timestamp,
        ),
    );
}

pub fn order_cancelled(env: &Env, order_id: u32, buyer: &Address, refunded_funds: i128) {
    let topics = (Symbol::new(env, "OrderCancelled"), order_id);
    let timestamp = env.ledger().timestamp();
    env.events()
        .publish(topics, (buyer.clone(), refunded_funds, timestamp));
}

pub fn order_matched(
    env: &Env,
    order_id: u32,
    invoice_id: &BytesN<32>,
    buyer: &Address,
    seller: &Address,
    price: i128,
    amount: i128,
) {
    let topics = (Symbol::new(env, "OrderMatched"), invoice_id.clone());
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            order_id,
            buyer.clone(),
            seller.clone(),
            price,
            amount,
            timestamp,
        ),
    );
}
