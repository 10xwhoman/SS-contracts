use crate::types::Sector;
use soroban_sdk::{Address, BytesN, Env, String, Symbol};

pub fn category_registered(
    env: &Env,
    invoice_id: &BytesN<32>,
    issuer: &Address,
    sector: Sector,
    sub_category: &String,
) {
    let topics = (
        Symbol::new(env, "CategoryRegistered"),
        invoice_id.clone(),
        sector as u32,
    );
    let timestamp = env.ledger().timestamp();
    env.events()
        .publish(topics, (issuer.clone(), sub_category.clone(), timestamp));
}
