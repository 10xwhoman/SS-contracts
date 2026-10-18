use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn rating_submitted(env: &Env, invoice_id: &BytesN<32>, wallet: &Address, score: u32) {
    let topics = (Symbol::new(env, "RatingSubmitted"), invoice_id.clone());
    env.events().publish(topics, (wallet.clone(), score));
}
