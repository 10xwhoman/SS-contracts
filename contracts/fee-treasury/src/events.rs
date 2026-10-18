use crate::types::SplitAmounts;
use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn fee_collected(
    env: &Env,
    invoice_id: &BytesN<32>,
    fee_amount: i128,
    splits: &SplitAmounts,
) {
    let topics = (Symbol::new(env, "FeeCollected"), invoice_id.clone());
    let timestamp = env.ledger().timestamp();
    env.events().publish(
        topics,
        (
            fee_amount,
            splits.protocol_amount,
            splits.issuer_amount,
            splits.lp_pool_amount,
            timestamp,
        ),
    );
}

pub fn fee_withdrawn(env: &Env, recipient: &Address, amount: i128) {
    let topics = (Symbol::new(env, "FeeWithdrawn"), recipient.clone());
    let timestamp = env.ledger().timestamp();
    env.events().publish(topics, (amount, timestamp));
}

pub fn split_ratios_updated(env: &Env, protocol_bps: u32, issuer_bps: u32, lp_pool_bps: u32) {
    let topics = (Symbol::new(env, "SplitRatiosUpdated"),);
    env.events()
        .publish(topics, (protocol_bps, issuer_bps, lp_pool_bps));
}
