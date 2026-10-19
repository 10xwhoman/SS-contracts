//! Tests for the investor position snapshot contract (issue #527).

#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    xdr::{ContractEvent, ContractEventBody, ScAddress, ScVal},
    Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

const START: u64 = 1_000_000;

struct Ctx<'a> {
    env: Env,
    admin: Address,
    snapshot: InvoiceSnapshotClient<'a>,
    inv1: Symbol,
    inv2: Symbol,
}

fn setup<'a>(env: &'a Env) -> Ctx<'a> {
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(env);
    let contract_id = env.register(InvoiceSnapshot, ());
    let snapshot = InvoiceSnapshotClient::new(env, &contract_id);
    snapshot.initialize(&admin);

    Ctx {
        env: env.clone(),
        admin,
        snapshot,
        inv1: Symbol::new(env, "INV1"),
        inv2: Symbol::new(env, "INV2"),
    }
}

fn parse_event(env: &Env, event: &ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => {
            Address::try_from_val(env, &ScVal::Address(ScAddress::Contract(hash.clone()))).unwrap()
        }
        None => Address::generate(env),
    };
    let ContractEventBody::V0(v0) = &event.body;
    let topics =
        Vec::<Val>::try_from_val(env, &ScVal::Vec(Some(v0.topics.clone().into()))).unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

fn topic_of(env: &Env, event: &ContractEvent) -> Symbol {
    let (_, topics, _) = parse_event(env, event);
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

fn balance_of(snapshot: &Snapshot, holder: &Address) -> i128 {
    for entry in snapshot.balances.iter() {
        if entry.holder == *holder {
            return entry.amount;
        }
    }
    0
}

// ── Snapshot capture ──────────────────────────────────────────────────────────

#[test]
fn take_snapshot_records_all_holder_balances() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &bob, &300);

    ctx.env.ledger().set_sequence_number(4_242);
    ctx.env.ledger().set_timestamp(START + 60);
    let snapshot_id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    assert_eq!(snapshot_id, 1);

    let snapshot = ctx.snapshot.get_snapshot(&ctx.inv1, &1);
    assert_eq!(snapshot.snapshot_id, 1);
    assert_eq!(snapshot.invoice_id, ctx.inv1);
    assert_eq!(snapshot.ledger_sequence, 4_242);
    assert_eq!(snapshot.timestamp, START + 60);
    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(balance_of(&snapshot, &alice), 500);
    assert_eq!(balance_of(&snapshot, &bob), 300);
    assert_eq!(snapshot.total_balance, 800);
}

#[test]
fn get_snapshot_returns_correct_balances_for_any_historical_snapshot() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    // Balance changes between snapshots must not rewrite history.
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &900);
    ctx.env.ledger().set_timestamp(START + 86_400);
    let second = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    assert_eq!(second, 2);

    let first_snapshot = ctx.snapshot.get_snapshot(&ctx.inv1, &1);
    assert_eq!(balance_of(&first_snapshot, &alice), 500);
    assert_eq!(first_snapshot.total_balance, 500);

    let second_snapshot = ctx.snapshot.get_snapshot(&ctx.inv1, &2);
    assert_eq!(balance_of(&second_snapshot, &alice), 900);
    assert_eq!(second_snapshot.total_balance, 900);
    assert_eq!(ctx.snapshot.get_latest_snapshot_id(&ctx.inv1), 2);
}

// ── Cross-invoice wallet history ──────────────────────────────────────────────

#[test]
fn get_investor_snapshots_returns_cross_invoice_data() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &bob, &200);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv2, &alice, &750);

    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    ctx.env.ledger().set_timestamp(START + 86_400);
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv2);

    let alice_history = ctx.snapshot.get_investor_snapshots(&alice);
    assert_eq!(alice_history.len(), 2);

    let first = alice_history.get(0).unwrap();
    assert_eq!(first.invoice_id, ctx.inv1);
    assert_eq!(first.snapshot_id, 1);
    assert_eq!(first.balance, 500);

    let second = alice_history.get(1).unwrap();
    assert_eq!(second.invoice_id, ctx.inv2);
    assert_eq!(second.snapshot_id, 1);
    assert_eq!(second.balance, 750);

    // Bob only holds in INV1.
    let bob_history = ctx.snapshot.get_investor_snapshots(&bob);
    assert_eq!(bob_history.len(), 1);
    assert_eq!(bob_history.get(0).unwrap().invoice_id, ctx.inv1);

    // A wallet with no positions gets an empty history.
    let stranger = Address::generate(&env);
    assert_eq!(ctx.snapshot.get_investor_snapshots(&stranger).len(), 0);
}

// ── Retention & pruning ───────────────────────────────────────────────────────

#[test]
fn snapshots_pruned_after_90_day_retention_period() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);

    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    // Exactly 90 days later it is still within retention.
    ctx.env.ledger().set_timestamp(START + RETENTION_SECS);
    assert!(ctx.snapshot.try_get_snapshot(&ctx.inv1, &1).is_ok());
    assert_eq!(ctx.snapshot.get_investor_snapshots(&alice).len(), 1);

    // One second past the window it expires.
    ctx.env.ledger().set_timestamp(START + RETENTION_SECS + 1);
    let result = ctx.snapshot.try_get_snapshot(&ctx.inv1, &1);
    assert_eq!(result, Err(Ok(Error::SnapshotNotFound)));
    assert_eq!(ctx.snapshot.get_investor_snapshots(&alice).len(), 0);

    // Explicit pruning removes the physical entry and wallet references.
    let pruned = ctx.snapshot.prune_snapshots(&ctx.inv1);
    assert_eq!(pruned, 1);
    assert_eq!(ctx.snapshot.get_snapshot_ids(&ctx.inv1).len(), 0);
    assert_eq!(ctx.snapshot.get_investor_snapshots(&alice).len(), 0);
    assert_eq!(ctx.snapshot.get_latest_snapshot_id(&ctx.inv1), 1);
}

#[test]
fn take_snapshot_auto_prunes_expired_snapshots() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);

    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    // 91 days later a fresh snapshot is taken: the expired one is pruned.
    ctx.env
        .ledger()
        .set_timestamp(START + RETENTION_SECS + 86_400);
    let fresh_id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    assert_eq!(fresh_id, 2);

    let result = ctx.snapshot.try_get_snapshot(&ctx.inv1, &1);
    assert_eq!(result, Err(Ok(Error::SnapshotNotFound)));

    let ids = ctx.snapshot.get_snapshot_ids(&ctx.inv1);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids.get(0).unwrap(), 2);

    // Wallet history only contains the fresh entry.
    let history = ctx.snapshot.get_investor_snapshots(&alice);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().snapshot_id, 2);
}

#[test]
fn prune_past_retention_ignores_live_snapshots() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &100);

    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    // 80 days: still live.
    ctx.env.ledger().set_timestamp(START + 80 * 86_400);
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    let pruned = ctx.snapshot.prune_snapshots(&ctx.inv1);
    assert_eq!(pruned, 0, "nothing should be pruned inside retention");

    // 91 days from the first snapshot: only snapshot 1 is expired.
    ctx.env
        .ledger()
        .set_timestamp(START + RETENTION_SECS + 86_400);
    let pruned = ctx.snapshot.prune_snapshots(&ctx.inv1);
    assert_eq!(pruned, 1);
    assert_eq!(ctx.snapshot.get_snapshot_ids(&ctx.inv1).len(), 1);
    assert!(ctx.snapshot.try_get_snapshot(&ctx.inv1, &2).is_ok());
}

// ── Events ────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_taken_event_includes_invoice_and_block_height() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);

    ctx.env.ledger().set_sequence_number(9_999);
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    let mut found = false;
    for e in env.events().all().events().iter() {
        if topic_of(&env, e) == Symbol::new(&env, "snapshot_taken") {
            found = true;
            let (_, _, data) = parse_event(&env, e);
            let (invoice_id, snapshot_id, ledger_sequence): (Symbol, u32, u32) =
                data.try_into_val(&env).unwrap();
            assert_eq!(invoice_id, ctx.inv1);
            assert_eq!(snapshot_id, 1);
            assert_eq!(ledger_sequence, 9_999);
        }
    }
    assert!(found, "expected SnapshotTaken event");
}

// ── Admin & registry guards ───────────────────────────────────────────────────

#[test]
fn snapshot_and_balance_updates_require_admin() {
    let env = Env::default();
    let ctx = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    let result = ctx
        .snapshot
        .try_set_holder_balance(&impostor, &ctx.inv1, &alice, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    let result = ctx.snapshot.try_take_snapshot(&impostor, &ctx.inv1);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn zero_balance_clears_holder_from_registry() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &100);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &bob, &200);
    assert_eq!(ctx.snapshot.get_holders(&ctx.inv1).len(), 2);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &0);
    let holders = ctx.snapshot.get_holders(&ctx.inv1);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders.get(0).unwrap(), bob);
    assert_eq!(ctx.snapshot.get_holder_balance(&ctx.inv1, &alice), 0);

    // Snapshot only captures live holders.
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    let snapshot = ctx.snapshot.get_snapshot(&ctx.inv1, &1);
    assert_eq!(snapshot.balances.len(), 1);
    assert_eq!(balance_of(&snapshot, &bob), 200);
}

#[test]
fn negative_balance_rejected_and_unknown_snapshot_not_found() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);

    let result = ctx
        .snapshot
        .try_set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &-1);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));

    let result = ctx.snapshot.try_get_snapshot(&ctx.inv1, &7);
    assert_eq!(result, Err(Ok(Error::SnapshotNotFound)));
}

#[test]
fn double_initialize_rejected() {
    let env = Env::default();
    let ctx = setup(&env);
    let result = ctx.snapshot.try_initialize(&ctx.admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

// ── Pro-rata distribution ─────────────────────────────────────────────────────

#[test]
fn get_snapshot_balance_ignores_post_snapshot_changes() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &bob, &300);
    let id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    // Balances move after the snapshot; the snapshot must not.
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &0);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &carol, &900);

    assert_eq!(ctx.snapshot.get_snapshot_balance(&ctx.inv1, &id, &alice), 500);
    assert_eq!(ctx.snapshot.get_snapshot_balance(&ctx.inv1, &id, &bob), 300);
    assert_eq!(ctx.snapshot.get_snapshot_balance(&ctx.inv1, &id, &carol), 0);
    assert_eq!(
        ctx.snapshot.try_get_snapshot_balance(&ctx.inv1, &99, &alice),
        Err(Ok(Error::SnapshotNotFound))
    );
}

#[test]
fn list_snapshots_returns_ids_and_timestamps() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &100);

    assert_eq!(ctx.snapshot.list_snapshots(&ctx.inv1).len(), 0);

    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    env.ledger().set_timestamp(START + 100);
    ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    let refs = ctx.snapshot.list_snapshots(&ctx.inv1);
    assert_eq!(refs.len(), 2);
    assert_eq!(refs.get(0).unwrap().snapshot_id, 1);
    assert_eq!(refs.get(0).unwrap().timestamp, START);
    assert_eq!(refs.get(1).unwrap().snapshot_id, 2);
    assert_eq!(refs.get(1).unwrap().timestamp, START + 100);
    assert_eq!(ctx.snapshot.list_snapshots(&ctx.inv2).len(), 0);
}

#[test]
fn distribute_from_snapshot_routes_pro_rata_amounts() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    StellarAssetClient::new(&env, &token).mint(&ctx.admin, &10_000);

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &500);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &bob, &300);
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &carol, &200);
    let id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);

    // Post-snapshot changes must not affect the split.
    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &1);

    let distributed = ctx
        .snapshot
        .distribute_from_snapshot(&ctx.admin, &ctx.inv1, &id, &token, &1_001);
    // Floored shares: 500, 300, 200 (of 1_001 * 50% / 30% / 20%).
    assert_eq!(distributed, 500 + 300 + 200);

    let client = TokenClient::new(&env, &token);
    assert_eq!(client.balance(&alice), 500);
    assert_eq!(client.balance(&bob), 300);
    assert_eq!(client.balance(&carol), 200);
    assert_eq!(client.balance(&ctx.admin), 10_000 - 1_000);

    let mut found = false;
    for e in env.events().all().events().iter() {
        if topic_of(&env, e) == Symbol::new(&env, "snapshot_distributed") {
            found = true;
        }
    }
    assert!(found, "expected snapshot_distributed event");
}

#[test]
fn distribute_from_snapshot_guards() {
    let env = Env::default();
    let ctx = setup(&env);
    let alice = Address::generate(&env);
    let impostor = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    ctx.snapshot
        .set_holder_balance(&ctx.admin, &ctx.inv1, &alice, &100);
    let id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv1);
    let empty_id = ctx.snapshot.take_snapshot(&ctx.admin, &ctx.inv2);

    assert_eq!(
        ctx.snapshot
            .try_distribute_from_snapshot(&impostor, &ctx.inv1, &id, &token, &100),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        ctx.snapshot
            .try_distribute_from_snapshot(&ctx.admin, &ctx.inv1, &id, &token, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        ctx.snapshot
            .try_distribute_from_snapshot(&ctx.admin, &ctx.inv1, &9, &token, &100),
        Err(Ok(Error::SnapshotNotFound))
    );
    assert_eq!(
        ctx.snapshot
            .try_distribute_from_snapshot(&ctx.admin, &ctx.inv2, &empty_id, &token, &100),
        Err(Ok(Error::NothingToDistribute))
    );
}
