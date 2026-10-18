use crate::types::PerInvoiceSummary;
use soroban_sdk::{Address, Env, Symbol, Vec};

pub fn batch_settlement_executed(
    env: &Env,
    caller: &Address,
    count: u32,
    summaries: &Vec<PerInvoiceSummary>,
) {
    let topics = (Symbol::new(env, "BatchSettlementExecuted"), caller.clone());
    env.events().publish(topics, (count, summaries.clone()));
}
