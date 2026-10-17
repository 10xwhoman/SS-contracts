#![allow(deprecated, clippy::all)]

use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use super::*;
use crate::errors;
use invoice_escrow::{InvoiceEscrow, InvoiceEscrowClient};

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

fn test_email_hash(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

/// Stand up the escrow (KYC whitelist) + waitlist contracts with shared admin.
fn setup(
    env: &Env,
) -> (
    Address,
    InvoiceEscrowClient<'static>,
    WaitlistClient<'static>,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);

    // Register and initialize the escrow contract (the KYC whitelist).
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);
    escrow.initialize(&admin, &300);

    // Register and initialize the waitlist contract.
    let waitlist_id = env.register(Waitlist, ());
    let waitlist = WaitlistClient::new(env, &waitlist_id);
    waitlist.initialize(&admin, &escrow_id);

    let escrow = unsafe { core::mem::transmute::<_, InvoiceEscrowClient<'static>>(escrow) };
    let waitlist = unsafe { core::mem::transmute::<_, WaitlistClient<'static>>(waitlist) };

    (admin, escrow, waitlist)
}

// ── join_waitlist tests ────────────────────────────────────────────────────

#[test]
fn test_join_waitlist_stores_in_correct_queue_order() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    let w3 = Address::generate(&env);

    waitlist.join_waitlist(&w1, &test_email_hash(&env, "a@test.com"));
    waitlist.join_waitlist(&w2, &test_email_hash(&env, "b@test.com"));
    waitlist.join_waitlist(&w3, &test_email_hash(&env, "c@test.com"));

    assert_eq!(waitlist.get_position(&w1), 1);
    assert_eq!(waitlist.get_position(&w2), 2);
    assert_eq!(waitlist.get_position(&w3), 3);
    assert_eq!(waitlist.get_waitlist_size(), 3);

    // Email hash is persisted alongside the queue position.
    env.as_contract(&waitlist.address, || {
        assert_eq!(
            crate::storage::get_wallet_email(&env, &w1),
            Some(test_email_hash(&env, "a@test.com"))
        );
        assert_eq!(
            crate::storage::get_wallet_email(&env, &w2),
            Some(test_email_hash(&env, "b@test.com"))
        );
    });
}

#[test]
fn test_join_waitlist_already_on_waitlist() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a@test.com"));

    let result = waitlist.try_join_waitlist(&wallet, &test_email_hash(&env, "b@test.com"));
    assert_eq!(result, Err(Ok(errors::Error::AlreadyOnWaitlist)));
}

#[test]
fn test_waitlist_joined_event() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a@test.com"));

    let events = env.events().all();
    let evt = events
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_, topics, _) = parse_event(&env, e);
            Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap()
                == Symbol::new(&env, "waitlist_joined")
        })
        .expect("expected waitlist_joined event");
    let (_, _, data) = parse_event(&env, evt);
    let (addr, pos): (Address, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(addr, wallet);
    assert_eq!(pos, 1);
}

// ── get_position tests ─────────────────────────────────────────────────────

#[test]
fn test_get_position_not_on_waitlist() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    assert_eq!(waitlist.get_position(&wallet), 0);
}

// ── approve_from_waitlist tests ────────────────────────────────────────────

#[test]
fn test_approve_from_waitlist_promotes_to_kyc_whitelist() {
    let env = Env::default();
    let (_admin, escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a@test.com"));

    // Before approval: not whitelisted.
    assert!(!escrow.is_buyer_whitelisted(&wallet));

    waitlist.approve_from_waitlist(&wallet);

    // After approval: whitelisted in the escrow contract.
    assert!(escrow.is_buyer_whitelisted(&wallet));
    assert!(waitlist.is_approved(&wallet));
}

#[test]
fn test_approve_from_waitlist_not_on_waitlist() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    let result = waitlist.try_approve_from_waitlist(&wallet);
    assert_eq!(result, Err(Ok(errors::Error::NotOnWaitlist)));
}

#[test]
fn test_approve_from_waitlist_already_approved() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a@test.com"));
    waitlist.approve_from_waitlist(&wallet);

    let result = waitlist.try_approve_from_waitlist(&wallet);
    assert_eq!(result, Err(Ok(errors::Error::AlreadyApproved)));
}

#[test]
fn test_waitlist_approved_event() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);
    env.ledger().set_timestamp(42_000);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a@test.com"));
    waitlist.approve_from_waitlist(&wallet);

    let events = env.events().all();
    let evt = events
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_, topics, _) = parse_event(&env, e);
            Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap()
                == Symbol::new(&env, "waitlist_approved")
        })
        .expect("expected waitlist_approved event");
    let (_, _, data) = parse_event(&env, evt);
    let (addr, ts): (Address, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(addr, wallet);
    assert_eq!(ts, 42_000);
}

// ── bulk_approve tests ─────────────────────────────────────────────────────

#[test]
fn test_bulk_approve_correct_top_n() {
    let env = Env::default();
    let (_admin, escrow, waitlist) = setup(&env);

    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    let w3 = Address::generate(&env);
    let w4 = Address::generate(&env);

    waitlist.join_waitlist(&w1, &test_email_hash(&env, "a"));
    waitlist.join_waitlist(&w2, &test_email_hash(&env, "b"));
    waitlist.join_waitlist(&w3, &test_email_hash(&env, "c"));
    waitlist.join_waitlist(&w4, &test_email_hash(&env, "d"));

    // Approve top 2 from queue.
    waitlist.bulk_approve(&2);

    // w1 and w2 should be approved and whitelisted.
    assert!(escrow.is_buyer_whitelisted(&w1));
    assert!(escrow.is_buyer_whitelisted(&w2));
    assert!(!escrow.is_buyer_whitelisted(&w3));
    assert!(!escrow.is_buyer_whitelisted(&w4));

    assert!(waitlist.is_approved(&w1));
    assert!(waitlist.is_approved(&w2));
    assert!(!waitlist.is_approved(&w3));
    assert!(!waitlist.is_approved(&w4));
}

#[test]
fn test_bulk_approve_skips_already_approved() {
    let env = Env::default();
    let (_admin, escrow, waitlist) = setup(&env);

    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    let w3 = Address::generate(&env);

    waitlist.join_waitlist(&w1, &test_email_hash(&env, "a"));
    waitlist.join_waitlist(&w2, &test_email_hash(&env, "b"));
    waitlist.join_waitlist(&w3, &test_email_hash(&env, "c"));

    // Approve w1 individually first.
    waitlist.approve_from_waitlist(&w1);

    // Bulk approve 2 — should skip w1 and approve w2, w3.
    waitlist.bulk_approve(&2);

    assert!(escrow.is_buyer_whitelisted(&w1));
    assert!(escrow.is_buyer_whitelisted(&w2));
    assert!(escrow.is_buyer_whitelisted(&w3));
}

#[test]
fn test_bulk_approve_zero_count_fails() {
    let env = Env::default();
    let (_admin, _escrow, waitlist) = setup(&env);

    let wallet = Address::generate(&env);
    waitlist.join_waitlist(&wallet, &test_email_hash(&env, "a"));

    let result = waitlist.try_bulk_approve(&0);
    assert_eq!(result, Err(Ok(errors::Error::InvalidCount)));
}

#[test]
fn test_bulk_approve_more_than_queue() {
    let env = Env::default();
    let (_admin, escrow, waitlist) = setup(&env);

    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);

    waitlist.join_waitlist(&w1, &test_email_hash(&env, "a"));
    waitlist.join_waitlist(&w2, &test_email_hash(&env, "b"));

    // Request 10 but only 2 in queue — should approve all 2.
    waitlist.bulk_approve(&10);

    assert!(escrow.is_buyer_whitelisted(&w1));
    assert!(escrow.is_buyer_whitelisted(&w2));
}

// ── Not-init errors ────────────────────────────────────────────────────────

#[test]
fn test_join_waitlist_not_init() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Waitlist, ());
    let waitlist = WaitlistClient::new(&env, &contract_id);

    let wallet = Address::generate(&env);
    let result = waitlist.try_join_waitlist(&wallet, &test_email_hash(&env, "a"));
    assert_eq!(result, Err(Ok(errors::Error::NotInit)));
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    escrow.initialize(&admin, &300);

    let waitlist_id = env.register(Waitlist, ());
    let waitlist = WaitlistClient::new(&env, &waitlist_id);
    waitlist.initialize(&admin, &escrow_id);

    let result = waitlist.try_initialize(&admin, &escrow_id);
    assert_eq!(result, Err(Ok(errors::Error::AlreadyInit)));
}
