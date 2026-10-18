use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn insurance_purchased(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    coverage_amount: i128,
    premium: i128,
) {
    let topics = (
        Symbol::new(env, "InsurancePurchased"),
        invoice_id.clone(),
        investor.clone(),
    );
    env.events().publish(topics, (coverage_amount, premium));
}

pub fn insurance_payout_triggered(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    payout: i128,
) {
    let topics = (
        Symbol::new(env, "InsurancePayoutTriggered"),
        invoice_id.clone(),
        investor.clone(),
    );
    env.events().publish(topics, payout);
}
