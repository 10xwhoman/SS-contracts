use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a new syndicate is created.
pub fn syndicate_created(
    env: &Env,
    syndicate_id: u32,
    invoice_id: &Symbol,
    lead_investor: &Address,
    target_amount: i128,
    funding_deadline: u64,
) {
    let topics = (Symbol::new(env, "SyndicateCreated"), syndicate_id);
    env.events()
        .publish(topics, (invoice_id.clone(), lead_investor.clone(), target_amount, funding_deadline));
}

/// Emitted when a co-investor joins a syndicate.
pub fn syndicate_joined(
    env: &Env,
    syndicate_id: u32,
    investor: &Address,
    amount: i128,
    total_raised: i128,
) {
    let topics = (Symbol::new(env, "SyndicateJoined"), syndicate_id);
    env.events()
        .publish(topics, (investor.clone(), amount, total_raised));
}

/// Emitted when the lead investor executes a fully funded syndicate.
pub fn syndicate_executed(
    env: &Env,
    syndicate_id: u32,
    invoice_id: &Symbol,
    total_raised: i128,
) {
    let topics = (Symbol::new(env, "SyndicateExecuted"), syndicate_id);
    env.events()
        .publish(topics, (invoice_id.clone(), total_raised));
}

/// Emitted when uncommitted funds are returned after an expired window.
pub fn syndicate_cancelled(env: &Env, syndicate_id: u32, total_returned: i128) {
    let topics = (Symbol::new(env, "SyndicateCancelled"), syndicate_id);
    env.events().publish(topics, total_returned);
}
