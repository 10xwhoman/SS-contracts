//! SEP-41 token interface compliance and authorization boundary test matrix (#441).
//!
//! Validates that the InvoiceToken contract implements every mandatory SEP-41
//! entry point correctly and that authorization boundaries are enforced — i.e.
//! operations requiring `from` auth reject unauthorized callers, zero-address
//! inputs are rejected, and edge cases (empty transfers, self-transfers,
//! expired allowances) behave as specified.

use super::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Symbol, Vec};

// ── Setup helpers ─────────────────────────────────────────────────────────────

fn setup_sep41_token(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address) {
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    let name = SorobanString::from_str(env, "SEP41 Test Token");
    let symbol = SorobanString::from_str(env, "SEP41T");
    let invoice_id = Symbol::new(env, "test_inv");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    (client, admin, minter)
}

// ── SEP-41 Required Interface Functions ───────────────────────────────────────

#[test]
fn sep41_name_returns_initialized_name() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    let name = client.name();
    assert_eq!(name, SorobanString::from_str(&env, "SEP41 Test Token"));
}

#[test]
fn sep41_symbol_returns_initialized_symbol() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    let symbol = client.symbol();
    assert_eq!(symbol, SorobanString::from_str(&env, "SEP41T"));
}

#[test]
fn sep41_decimals_returns_configured_decimals() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    assert_eq!(client.decimals(), 7);
}

#[test]
fn sep41_total_supply_starts_at_zero() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn sep41_balance_of_unfunded_account_is_zero() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    let user = Address::generate(&env);
    assert_eq!(client.balance(&user), 0);
}

#[test]
fn sep41_allowance_of_unapproved_pair_is_zero() {
    let env = Env::default();
    let (client, _, _) = setup_sep41_token(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    assert_eq!(client.allowance(&owner, &spender), 0);
}

// ── Authorization boundary tests ──────────────────────────────────────────────

#[test]
fn transfer_requires_from_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    // Mint tokens to admin
    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let recipient = Address::generate(&env);

    // Without mocking auth for admin, transfer should fail
    env.mock_auths(&[]);
    let result = client.try_transfer(&admin, &recipient, &100);
    assert!(result.is_err(), "transfer must fail without from auth");
}

#[test]
fn transfer_from_requires_from_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    client.approve(&admin, &spender, &500, &1000);

    let recipient = Address::generate(&env);

    // Without auth for admin (the from), transfer_from should fail
    env.mock_auths(&[]);
    let result = client.try_transfer_from(&spender, &admin, &recipient, &100);
    assert!(result.is_err(), "transfer_from must fail without from auth");
}

#[test]
fn approve_requires_from_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);

    // Without auth for admin, approve should fail
    env.mock_auths(&[]);
    let result = client.try_approve(&admin, &spender, &500, &1000);
    assert!(result.is_err(), "approve must fail without from auth");
}

#[test]
fn burn_requires_from_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    // Without auth for admin, burn should fail
    env.mock_auths(&[]);
    let result = client.try_burn(&admin, &100);
    assert!(result.is_err(), "burn must fail without from auth");
}

#[test]
fn burn_from_requires_from_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    client.approve(&admin, &spender, &500, &1000);

    // Without auth for admin (the from), burn_from should fail
    env.mock_auths(&[]);
    let result = client.try_burn_from(&spender, &admin, &100);
    assert!(result.is_err(), "burn_from must fail without from auth");
}

// ── Edge case: self-transfer ──────────────────────────────────────────────────

#[test]
fn transfer_to_self_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let result = client.try_transfer(&admin, &admin, &100);
    assert!(result.is_err(), "self-transfer should be rejected");
}

// ── Edge case: zero amount ────────────────────────────────────────────────────

#[test]
fn transfer_zero_amount_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&admin, &recipient, &0);
    assert!(result.is_err(), "zero-amount transfer should be rejected");
}

#[test]
fn transfer_negative_amount_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&admin, &recipient, &-1);
    assert!(
        result.is_err(),
        "negative-amount transfer should be rejected"
    );
}

// ── Edge case: insufficient balance ───────────────────────────────────────────

#[test]
fn transfer_exceeding_balance_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &100, &minter);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&admin, &recipient, &200);
    assert!(
        result.is_err(),
        "transfer exceeding balance should be rejected"
    );
}

// ── Edge case: expired allowance ──────────────────────────────────────────────

#[test]
fn transfer_from_with_expired_allowance_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Approve with expiration at current ledger
    env.ledger().with_mut(|li| {
        li.sequence = 100;
    });
    client.approve(&admin, &spender, &500, &100);

    // Advance ledger past expiration
    env.ledger().with_mut(|li| {
        li.sequence = 101;
    });

    let result = client.try_transfer_from(&spender, &admin, &recipient, &100);
    assert!(
        result.is_err(),
        "transfer_from with expired allowance should fail"
    );
}

// ── Edge case: allowance exceeded ─────────────────────────────────────────────

#[test]
fn transfer_from_exceeding_allowance_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.approve(&admin, &spender, &50, &1000);

    let result = client.try_transfer_from(&spender, &admin, &recipient, &100);
    assert!(
        result.is_err(),
        "transfer_from exceeding allowance should fail"
    );
}

// ── Zero address rejection ────────────────────────────────────────────────────

#[test]
fn transfer_to_zero_address_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);

    let zero = soroban_sdk::Address::try_from_val(
        &env,
        &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Account(
            soroban_sdk::xdr::AccountId(soroban_sdk::xdr::PublicKey::PublicKeyTypeEd25519(
                soroban_sdk::xdr::Uint256([0; 32]),
            )),
        )),
    )
    .unwrap();

    let result = client.try_transfer(&admin, &zero, &100);
    assert!(
        result.is_err(),
        "transfer to zero address should be rejected"
    );
}

// ── Initialization guard ──────────────────────────────────────────────────────

#[test]
fn double_initialization_is_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    let name2 = SorobanString::from_str(&env, "Duplicate");
    let symbol2 = SorobanString::from_str(&env, "DUP");
    let invoice_id2 = Symbol::new(&env, "dup_inv");

    let result = client.try_initialize(&admin, &name2, &symbol2, &7u32, &invoice_id2, &minter);
    assert!(result.is_err(), "double initialization should be rejected");
}

// ── Mint authorization boundary ───────────────────────────────────────────────

#[test]
fn mint_requires_minter_auth() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    let recipient = Address::generate(&env);

    // Without minter auth, mint should fail
    env.mock_auths(&[]);
    let result = client.try_mint(&recipient, &100, &minter);
    assert!(result.is_err(), "mint must fail without minter auth");
}

// ── Pause enforcement ─────────────────────────────────────────────────────────

#[test]
fn transfer_rejected_when_paused() {
    let env = Env::default();
    let (client, admin, minter) = setup_sep41_token(&env);

    env.mock_all_auths();
    client.mint(&admin, &1000, &minter);
    client.pause(&admin);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&admin, &recipient, &100);
    assert!(result.is_err(), "transfer should be rejected when paused");
}
