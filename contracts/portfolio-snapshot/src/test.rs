#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

use crate::{Error, PortfolioSnapshot, PortfolioSnapshotClient};

const DAY: u64 = 24 * 60 * 60;
const RETENTION: u64 = 90 * DAY;

/// Lightweight mock fraction-token that stores balances in instance storage.
/// Avoids the real InvoiceToken's `extend_ttl` on non-existent balance keys.
#[contract]
pub struct MockFractionToken;

#[contractimpl]
impl MockFractionToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        let current: i128 = env.storage().instance().get(&to).unwrap_or(0);
        env.storage().instance().set(&to, &(current + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().instance().get(&id).unwrap_or(0)
    }
}

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

fn setup<'a>(env: &Env) -> (Address, PortfolioSnapshotClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PortfolioSnapshot);
    let client = PortfolioSnapshotClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
}

/// Deploy a mock fraction-token contract and mint `amount` fractions to `holder`.
fn deploy_mock_token(env: &Env, holder: &Address, amount: i128) -> Address {
    let token_id = env.register_contract(None, MockFractionToken);
    if amount > 0 {
        let client = MockFractionTokenClient::new(env, &token_id);
        let minter = Address::generate(env);
        client.mint(holder, &amount, &minter);
    }
    token_id
}

#[test]
fn test_snapshot_records_all_invoice_balances_for_wallet() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv_one = Symbol::new(&env, "INV_ONE");
    let inv_two = Symbol::new(&env, "INV_TWO");
    let inv_zero = Symbol::new(&env, "INV_ZERO");
    let wallet = Address::generate(&env);

    let token_one = deploy_mock_token(&env, &wallet, 1_000);
    let token_two = deploy_mock_token(&env, &wallet, 2_500);
    // Wallet holds no fractions in this invoice: must be omitted from the map.
    let token_zero = deploy_mock_token(&env, &wallet, 0);

    client.register_token(&admin, &inv_one, &token_one);
    client.register_token(&admin, &inv_two, &token_two);
    client.register_token(&admin, &inv_zero, &token_zero);

    env.ledger().set_timestamp(1_000_000);
    let snapshot_id = client.take_portfolio_snapshot(&wallet);
    assert_eq!(snapshot_id, 1);

    let snapshot = client.get_portfolio_snapshot(&wallet, &snapshot_id);
    assert_eq!(snapshot.snapshot_id, 1);
    assert_eq!(snapshot.timestamp, 1_000_000);
    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(snapshot.balances.get(inv_one.clone()).unwrap(), 1_000);
    assert_eq!(snapshot.balances.get(inv_two.clone()).unwrap(), 2_500);
    assert_eq!(snapshot.balances.get(inv_zero.clone()), None);
}

#[test]
fn test_historical_snapshot_is_immutable_after_balance_changes() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_HIST");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 1_000);
    client.register_token(&admin, &inv, &token);

    env.ledger().set_timestamp(500_000);
    let first = client.take_portfolio_snapshot(&wallet);

    // Balance changes after the first snapshot.
    let token_client = MockFractionTokenClient::new(&env, &token);
    let minter = Address::generate(&env);
    token_client.mint(&wallet, &500, &minter);

    env.ledger().set_timestamp(600_000);
    let second = client.take_portfolio_snapshot(&wallet);
    assert_eq!(first, 1);
    assert_eq!(second, 2);

    // Historical snapshot still reflects the original balance.
    let old = client.get_portfolio_snapshot(&wallet, &first);
    assert_eq!(old.balances.get(inv.clone()).unwrap(), 1_000);
    assert_eq!(old.timestamp, 500_000);

    let new = client.get_portfolio_snapshot(&wallet, &second);
    assert_eq!(new.balances.get(inv.clone()).unwrap(), 1_500);
    assert_eq!(new.timestamp, 600_000);
}

#[test]
fn test_list_snapshots_returns_ids_and_timestamps_in_order() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_LIST");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 10);
    client.register_token(&admin, &inv, &token);

    for (index, ts) in [100u64, 200, 300].iter().enumerate() {
        env.ledger().set_timestamp(*ts);
        let id = client.take_portfolio_snapshot(&wallet);
        assert_eq!(id, index as u64 + 1);
    }

    let list = client.list_snapshots(&wallet);
    assert_eq!(list.len(), 3);
    for (index, info) in list.iter().enumerate() {
        assert_eq!(info.snapshot_id, index as u64 + 1);
        assert_eq!(info.timestamp, (index as u64 + 1) * 100);
    }
    assert_eq!(client.get_snapshot_count(&wallet), 3);

    // Wallets with no snapshots list empty.
    let stranger = Address::generate(&env);
    assert_eq!(client.list_snapshots(&stranger).len(), 0);
}

#[test]
fn test_retention_keeps_snapshot_at_exactly_90_days_and_prunes_after() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_RET");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 42);
    client.register_token(&admin, &inv, &token);

    env.ledger().set_timestamp(1_000_000);
    let snapshot_id = client.take_portfolio_snapshot(&wallet);

    // Exactly at the retention boundary the snapshot is still retained.
    env.ledger().set_timestamp(1_000_000 + RETENTION);
    assert_eq!(client.prune_snapshots(&wallet), 0);
    assert_eq!(client.get_snapshot_count(&wallet), 1);

    // One second past the boundary it is pruned.
    env.ledger().set_timestamp(1_000_000 + RETENTION + 1);
    assert_eq!(client.prune_snapshots(&wallet), 1);
    assert_eq!(client.get_snapshot_count(&wallet), 0);
    assert_eq!(client.list_snapshots(&wallet).len(), 0);
    assert_eq!(
        client.try_get_portfolio_snapshot(&wallet, &snapshot_id),
        Err(Ok(Error::SnapshotNotFound))
    );
}

#[test]
fn test_new_snapshot_auto_prunes_expired_ones() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_AUTO");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 7);
    client.register_token(&admin, &inv, &token);

    env.ledger().set_timestamp(1_000);
    let old_id = client.take_portfolio_snapshot(&wallet);

    env.ledger().set_timestamp(1_000 + RETENTION + 1);
    let new_id = client.take_portfolio_snapshot(&wallet);

    assert_eq!(old_id, 1);
    assert_eq!(new_id, 2);
    assert_eq!(client.get_snapshot_count(&wallet), 1);
    let list = client.list_snapshots(&wallet);
    assert_eq!(list.get(0).unwrap().snapshot_id, 2);
    assert_eq!(
        client.try_get_portfolio_snapshot(&wallet, &old_id),
        Err(Ok(Error::SnapshotNotFound))
    );
    // The fresh snapshot remains retrievable.
    let fresh = client.get_portfolio_snapshot(&wallet, &new_id);
    assert_eq!(fresh.balances.get(inv.clone()).unwrap(), 7);
}

#[test]
fn test_take_daily_snapshots_admin_only_records_all_wallets() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_DAY");
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let token = deploy_mock_token(&env, &first, 100);
    client.register_token(&admin, &inv, &token);

    env.ledger().set_timestamp(900_000);
    let wallets = soroban_sdk::vec![&env, first.clone(), second.clone()];
    let ids = client.take_daily_snapshots(&admin, &wallets);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 1);

    assert_eq!(client.get_snapshot_count(&first), 1);
    assert_eq!(client.get_snapshot_count(&second), 1);
    assert_eq!(
        client
            .get_portfolio_snapshot(&first, &1)
            .balances
            .get(inv.clone())
            .unwrap(),
        100
    );
    // `second` holds nothing: snapshot is recorded but empty.
    assert_eq!(client.get_portfolio_snapshot(&second, &1).balances.len(), 0);

    // Non-admin callers are rejected.
    let impostor = Address::generate(&env);
    assert_eq!(
        client.try_take_daily_snapshots(&impostor, &wallets),
        Err(Ok(Error::Unauthorized))
    );

    // Missing admin auth is rejected.
    env.set_auths(&[]);
    assert!(client
        .try_take_daily_snapshots(&admin, &wallets)
        .is_err());
}

#[test]
fn test_portfolio_snapshot_taken_event_contains_wallet_and_timestamp() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_EVT");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 5);
    client.register_token(&admin, &inv, &token);

    env.ledger().set_timestamp(123_456);
    client.take_portfolio_snapshot(&wallet);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(contract_addr, client.address);

    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        Symbol::new(&env, "PortfolioSnapshotTaken")
    );
    let event_wallet: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_wallet, wallet);

    let (snapshot_id, timestamp): (u64, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(snapshot_id, 1);
    assert_eq!(timestamp, 123_456);
}

#[test]
fn test_token_registration_is_admin_only_and_replaces_existing() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_REG");
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    client.register_token(&admin, &inv, &token_a);
    assert_eq!(client.list_tokens().len(), 1);
    assert_eq!(client.list_tokens().get(0).unwrap().token, token_a);

    // Re-registering the same invoice replaces the token address in place.
    client.register_token(&admin, &inv, &token_b);
    let tokens = client.list_tokens();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get(0).unwrap().token, token_b);

    let impostor = Address::generate(&env);
    assert_eq!(
        client.try_register_token(&impostor, &inv, &token_a),
        Err(Ok(Error::Unauthorized))
    );

    env.set_auths(&[]);
    assert!(client.try_register_token(&admin, &inv, &token_a).is_err());
}

#[test]
fn test_remove_token_stops_tracking() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let inv = Symbol::new(&env, "INV_RM");
    let wallet = Address::generate(&env);
    let token = deploy_mock_token(&env, &wallet, 9);
    client.register_token(&admin, &inv, &token);

    client.remove_token(&admin, &inv);
    assert_eq!(client.list_tokens().len(), 0);

    // Snapshot no longer includes the removed invoice.
    client.take_portfolio_snapshot(&wallet);
    let snapshot = client.get_portfolio_snapshot(&wallet, &1);
    assert_eq!(snapshot.balances.len(), 0);

    assert_eq!(
        client.try_remove_token(&admin, &inv),
        Err(Ok(Error::TokenNotFound))
    );
}

#[test]
fn test_failing_token_contract_is_skipped() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    // Registered address has no contract deployed: the balance call fails and
    // the snapshot should still be recorded without that invoice.
    let broken = Address::generate(&env);
    client.register_token(&admin, &Symbol::new(&env, "INV_BAD"), &broken);

    let wallet = Address::generate(&env);
    let snapshot_id = client.take_portfolio_snapshot(&wallet);
    assert_eq!(snapshot_id, 1);
    assert_eq!(
        client.get_portfolio_snapshot(&wallet, &snapshot_id).balances.len(),
        0
    );
}

#[test]
fn test_uninitialized_and_invalid_inputs_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PortfolioSnapshot);
    let client = PortfolioSnapshotClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invoice = Symbol::new(&env, "INV_INIT");
    let token = Address::generate(&env);
    assert_eq!(
        client.try_register_token(&admin, &invoice, &token),
        Err(Ok(Error::NotInit))
    );
    assert_eq!(client.try_get_admin(), Err(Ok(Error::NotInit)));

    client.initialize(&admin);
    assert_eq!(client.try_initialize(&admin), Err(Ok(Error::AlreadyInit)));

    let zero_str = String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    assert_eq!(
        client.try_take_portfolio_snapshot(&zero),
        Err(Ok(Error::InvalidAddress))
    );

    // Unknown snapshot id.
    let wallet = Address::generate(&env);
    assert_eq!(
        client.try_get_portfolio_snapshot(&wallet, &99),
        Err(Ok(Error::SnapshotNotFound))
    );

    // Retention window is exposed for clients.
    assert_eq!(client.get_retention_secs(), RETENTION);
}
