use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn watched_invoice_state_changed(
    env: &Env,
    invoice_id: &BytesN<32>,
    watcher: &Address,
    old_state: u32,
    new_state: u32,
) {
    let topics = (
        Symbol::new(env, "WatchedInvoiceStateChanged"),
        invoice_id.clone(),
    );
    let timestamp = env.ledger().timestamp();
    env.events()
        .publish(topics, (watcher.clone(), old_state, new_state, timestamp));
}

pub fn invoice_added_to_watchlist(env: &Env, wallet: &Address, invoice_id: &BytesN<32>) {
    let topics = (Symbol::new(env, "InvoiceAddedToWatchlist"), wallet.clone());
    env.events().publish(topics, invoice_id.clone());
}

pub fn invoice_removed_from_watchlist(env: &Env, wallet: &Address, invoice_id: &BytesN<32>) {
    let topics = (Symbol::new(env, "InvoiceRemovedFromWatchlist"), wallet.clone());
    env.events().publish(topics, invoice_id.clone());
}
