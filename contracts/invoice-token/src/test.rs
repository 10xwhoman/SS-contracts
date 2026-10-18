#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]
//! Unit tests for the invoice token contract.

use super::{InvoiceToken, InvoiceTokenClient};
use crate::types::StorageKey;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

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

fn setup_token(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address) {
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    let name = SorobanString::from_str(env, "Invoice INV-001");
    let symbol = SorobanString::from_str(env, "INV001");
    let invoice_id = Symbol::new(env, "inv_001");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    (client, admin, minter)
}

fn zero_account_address(env: &Env) -> Address {
    Address::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Account(
            soroban_sdk::xdr::AccountId(soroban_sdk::xdr::PublicKey::PublicKeyTypeEd25519(
                soroban_sdk::xdr::Uint256([0; 32]),
            )),
        )),
    )
    .unwrap()
}

fn zero_contract_address(env: &Env) -> Address {
    Address::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(
            soroban_sdk::xdr::ContractId(soroban_sdk::xdr::Hash([0; 32])),
        )),
    )
    .unwrap()
}

macro_rules! assert_invalid_address {
    ($result:expr) => {
        assert_eq!($result, Err(Ok(crate::errors::Error::InvalidAddress)))
    };
}

#[test]
fn test_zero_address_detection_covers_account_and_contract_addresses() {
    let env = Env::default();

    assert!(crate::types::is_zero_address(
        &env,
        &zero_account_address(&env)
    ));
    assert!(crate::types::is_zero_address(
        &env,
        &zero_contract_address(&env)
    ));
    assert!(!crate::types::is_zero_address(
        &env,
        &Address::generate(&env)
    ));
}

#[test]
fn test_initialize_rejects_zero_admin_and_minter() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-001");
    let symbol = SorobanString::from_str(&env, "INV001");
    let invoice_id = Symbol::new(&env, "inv_001");

    assert_invalid_address!(client.try_initialize(
        &zero_account_address(&env),
        &name,
        &symbol,
        &7,
        &invoice_id,
        &minter,
    ));
    assert_invalid_address!(client.try_initialize(
        &admin,
        &name,
        &symbol,
        &7,
        &invoice_id,
        &zero_contract_address(&env),
    ));
}

#[test]
fn test_address_queries_reject_zero_addresses() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);
    let zero = zero_contract_address(&env);
    let role = Symbol::new(&env, "admin");

    assert_invalid_address!(client.try_balance(&zero));

    let ids = soroban_sdk::vec![&env, admin.clone(), zero.clone()];
    assert_invalid_address!(client.try_balance_batch(&ids));

    assert_invalid_address!(client.try_allowance(&zero, &admin));
    assert_invalid_address!(client.try_allowance(&admin, &zero));
    assert_invalid_address!(client.try_get_nonce(&zero));
    assert_invalid_address!(client.try_is_account_frozen(&zero));
    assert_invalid_address!(client.try_is_frozen(&zero));
    assert_invalid_address!(client.try_has_role(&role, &zero));
    assert_invalid_address!(client.try_get_token_history(&zero));
}

#[test]
fn test_token_operations_reject_every_zero_address_parameter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    let other = Address::generate(&env);
    let zero = zero_account_address(&env);
    let expiration = env.ledger().sequence() + 100;

    assert_invalid_address!(client.try_transfer(&zero, &account, &1));
    assert_invalid_address!(client.try_transfer(&account, &zero, &1));

    assert_invalid_address!(client.try_approve(&zero, &account, &1, &expiration));
    assert_invalid_address!(client.try_approve(&account, &zero, &1, &expiration));
    assert_invalid_address!(client.try_extend_allowance(&zero, &account, &expiration));
    assert_invalid_address!(client.try_extend_allowance(&account, &zero, &expiration));
    assert_invalid_address!(client.try_revoke_approval(&zero, &account));
    assert_invalid_address!(client.try_revoke_approval(&account, &zero));

    assert_invalid_address!(client.try_transfer_from(&zero, &account, &other, &1));
    assert_invalid_address!(client.try_transfer_from(&account, &zero, &other, &1));
    assert_invalid_address!(client.try_transfer_from(&account, &other, &zero, &1));

    assert_invalid_address!(client.try_burn(&zero, &1));
    assert_invalid_address!(client.try_burn_from(&zero, &account, &1));
    assert_invalid_address!(client.try_burn_from(&account, &zero, &1));

    assert_invalid_address!(client.try_mint(&zero, &1, &minter));
    assert_invalid_address!(client.try_mint(&account, &1, &zero));

    let recipients = soroban_sdk::vec![&env, account.clone(), zero.clone()];
    let amounts = soroban_sdk::vec![&env, 1i128, 1i128];
    assert_invalid_address!(client.try_mint_batch(&recipients, &amounts, &minter));

    let recipients = soroban_sdk::vec![&env, account.clone()];
    let amounts = soroban_sdk::vec![&env, 1i128];
    assert_invalid_address!(client.try_mint_batch(&recipients, &amounts, &zero));

    assert_invalid_address!(client.try_set_transfer_locked(&zero, &false));
    assert_invalid_address!(client.try_set_minter(&zero));

    assert_invalid_address!(client.try_freeze_account(&zero));
    assert_invalid_address!(client.try_unfreeze_account(&zero));
    assert_invalid_address!(client.try_set_fee_bps(&zero, &100));

    let role = Symbol::new(&env, "admin");
    assert_invalid_address!(client.try_set_role_admin(&zero, &role, &account));
    assert_invalid_address!(client.try_set_role_admin(&admin, &role, &zero));
    assert_invalid_address!(client.try_grant_role(&zero, &role, &account));
    assert_invalid_address!(client.try_grant_role(&admin, &role, &zero));
    assert_invalid_address!(client.try_revoke_role(&zero, &role, &account));
    assert_invalid_address!(client.try_revoke_role(&admin, &role, &zero));
}

// ========== Original Tests ==========

#[test]
fn test_initialize_and_metadata() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.balance(&admin), 0);
    assert_eq!(client.invoice_id(), Symbol::new(&env, "inv_001"));
    assert!(client.transfer_locked());

    let other = Address::generate(&env);
    assert_eq!(client.balance(&other), 0);
    assert_eq!(client.allowance(&admin, &other), 0);
}

#[test]
fn test_transfer_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    assert_eq!(client.balance(&user), 1000);
    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    assert_eq!(client.balance(&admin), 1000);
    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
}

#[test]
fn test_transfer_from_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user, &spender, &500, &expiration);

    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let result = client.try_transfer_from(&spender, &user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_from_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.allowance(&admin, &spender), 400);
}

#[test]
fn test_transfer_unlocked_all_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint(&user1, &1000, &minter);
    client.mint(&user2, &1000, &minter);

    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());

    let recipient = Address::generate(&env);
    client.transfer(&user1, &recipient, &100);
    assert_eq!(client.balance(&user1), 900);
    assert_eq!(client.balance(&recipient), 100);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user2, &spender, &500, &expiration);

    let recipient2 = Address::generate(&env);
    client.transfer_from(&spender, &user2, &recipient2, &200);
    assert_eq!(client.balance(&user2), 800);
    assert_eq!(client.balance(&recipient2), 200);
    assert_eq!(client.allowance(&user2, &spender), 300);
}

#[test]
fn test_set_transfer_locked_toggle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    client.set_transfer_locked(&admin, &false);
    assert!(!client.transfer_locked());
    client.transfer(&user, &recipient, &100);
    assert_eq!(client.balance(&user), 900);
    assert_eq!(client.balance(&recipient), 100);

    client.set_transfer_locked(&admin, &true);
    assert!(client.transfer_locked());
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_set_transfer_locked_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "transfer_locked_updated"),).into_val(&env)
    );
    let event_data: (bool, bool) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (true, false));
}

#[test]
fn test_set_transfer_locked_by_minter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    assert!(client.transfer_locked());
    client.set_transfer_locked(&minter, &false);
    assert!(!client.transfer_locked());

    client.set_transfer_locked(&minter, &true);
    assert!(client.transfer_locked());
}

#[test]
fn test_set_minter_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, old_minter) = setup_token(&env);

    let new_minter = Address::generate(&env);
    client.set_minter(&new_minter);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "minter_updated"),).into_val(&env)
    );
    let event_data: (Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, old_minter);
    assert_eq!(event_data.1, new_minter);
}

#[test]
fn test_set_transfer_locked_unauthorized_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let stranger = Address::generate(&env);
    let result = client.try_set_transfer_locked(&stranger, &false);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
    assert!(client.transfer_locked());
}

// ========== Account Freeze Tests ==========

#[test]
fn test_freeze_account_requires_admin_auth() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);
    let account = Address::generate(&env);

    assert!(client.try_freeze_account(&account).is_err());
    assert!(!client.is_account_frozen(&account));
}

#[test]
fn test_freeze_unfreeze_account_lifecycle_and_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);
    let account = Address::generate(&env);

    client.freeze_account(&account);

    let freeze_event = env.events().all().events().last().unwrap().clone();
    assert!(client.is_frozen(&account));
    let (_contract_addr, topics, data) = parse_event(&env, &freeze_event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "account_frozen"),).into_val(&env)
    );
    let event_account: Address = data.try_into_val(&env).unwrap();
    assert_eq!(event_account, account);

    assert_eq!(
        client.try_freeze_account(&account),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert!(client.is_account_frozen(&account));

    client.unfreeze_account(&account);

    let unfreeze_event = env.events().all().events().last().unwrap().clone();
    assert!(!client.is_frozen(&account));
    let (_contract_addr, topics, data) = parse_event(&env, &unfreeze_event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "account_unfrozen"),).into_val(&env)
    );
    let event_account: Address = data.try_into_val(&env).unwrap();
    assert_eq!(event_account, account);

    assert_eq!(
        client.try_unfreeze_account(&account),
        Err(Ok(crate::errors::Error::AccountNotFrozen))
    );
    assert!(!client.is_account_frozen(&account));
}

#[test]
fn test_frozen_accounts_cannot_transfer_or_receive() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.set_transfer_locked(&admin, &false);
    client.mint(&sender, &100, &minter);
    client.freeze_account(&sender);

    assert_eq!(
        client.try_transfer(&sender, &recipient, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(client.balance(&sender), 100);
    assert_eq!(client.balance(&recipient), 0);

    client.unfreeze_account(&sender);
    client.freeze_account(&recipient);
    assert_eq!(
        client.try_transfer(&sender, &recipient, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(client.balance(&sender), 100);
    assert_eq!(client.balance(&recipient), 0);

    client.unfreeze_account(&recipient);
    client.transfer(&sender, &recipient, &10);
    assert_eq!(client.balance(&sender), 90);
    assert_eq!(client.balance(&recipient), 10);
}

#[test]
fn test_frozen_accounts_cannot_approve_burn_or_use_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    client.set_transfer_locked(&admin, &false);
    client.mint(&owner, &100, &minter);
    client.approve(&owner, &spender, &50, &expiration);
    client.freeze_account(&owner);

    assert_eq!(
        client.try_approve(&owner, &spender, &25, &expiration),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(
        client.try_burn(&owner, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(
        client.try_transfer_from(&spender, &owner, &recipient, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(
        client.try_burn_from(&spender, &owner, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(client.balance(&owner), 100);
    assert_eq!(client.allowance(&owner, &spender), 50);

    client.unfreeze_account(&owner);
    client.freeze_account(&spender);
    assert_eq!(
        client.try_transfer_from(&spender, &owner, &recipient, &10),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(
        client.try_extend_allowance(&owner, &spender, &(expiration + 1)),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
}

#[test]
fn test_mint_rejects_frozen_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let frozen = Address::generate(&env);
    let active = Address::generate(&env);

    client.freeze_account(&frozen);
    assert_eq!(
        client.try_mint(&frozen, &10, &minter),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    client.freeze_account(&minter);
    assert_eq!(
        client.try_mint(&active, &10, &minter),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );

    let recipients = soroban_sdk::vec![&env, active.clone(), frozen.clone()];
    let amounts = soroban_sdk::vec![&env, 10i128, 20i128];
    assert_eq!(
        client.try_mint_batch(&recipients, &amounts, &minter),
        Err(Ok(crate::errors::Error::AccountFrozen))
    );
    assert_eq!(client.balance(&active), 0);
    assert_eq!(client.balance(&frozen), 0);
    assert_eq!(client.total_supply(), 0);
}

// ========== Clawback Tests (Issue #483: SEP-41 Regulatory Compliance) ==========

#[test]
fn test_clawback_requires_admin_auth() {
    let env = Env::default();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    // Mint some tokens and freeze account
    env.mock_all_auths();
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    env.mock_all_auths_allowing_non_root_auth();
    
    // Non-admin cannot clawback
    let stranger = Address::generate(&env);
    assert!(client.try_clawback(&account, &50).is_err());
}

#[test]
fn test_clawback_only_works_on_frozen_accounts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    
    // Cannot clawback from unfrozen account
    assert_eq!(
        client.try_clawback(&account, &50),
        Err(Ok(crate::errors::Error::AccountNotFrozen))
    );
    assert_eq!(client.balance(&account), 100);
}

#[test]
fn test_clawback_full_balance_from_frozen_account() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    
    let supply_before = client.total_supply();
    client.clawback(&account, &100);
    
    assert_eq!(client.balance(&account), 0);
    assert_eq!(client.total_supply(), supply_before - 100);
}

#[test]
fn test_clawback_partial_balance_from_frozen_account() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    
    let supply_before = client.total_supply();
    client.clawback(&account, &60);
    
    assert_eq!(client.balance(&account), 40);
    assert_eq!(client.total_supply(), supply_before - 60);
    
    // Can clawback again
    client.clawback(&account, &40);
    assert_eq!(client.balance(&account), 0);
    assert_eq!(client.total_supply(), supply_before - 100);
}

#[test]
fn test_clawback_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    
    assert_eq!(
        client.try_clawback(&account, &150),
        Err(Ok(crate::errors::Error::InsufficientBalance))
    );
    assert_eq!(client.balance(&account), 100);
}

#[test]
fn test_clawback_zero_or_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    
    assert_eq!(
        client.try_clawback(&account, &0),
        Err(Ok(crate::errors::Error::InvalidAmount))
    );
    assert_eq!(
        client.try_clawback(&account, &-10),
        Err(Ok(crate::errors::Error::InvalidAmount))
    );
    assert_eq!(client.balance(&account), 100);
}

#[test]
fn test_clawback_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    client.clawback(&account, &60);
    
    let clawback_event = env.events().all().events().last().unwrap().clone();
    let (_contract_addr, topics, data) = parse_event(&env, &clawback_event);
    
    assert_eq!(
        topics,
        (Symbol::new(&env, "clawback"), account.clone()).into_val(&env)
    );
    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 60);
}

#[test]
fn test_clawback_multiple_frozen_accounts_independence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let account1 = Address::generate(&env);
    let account2 = Address::generate(&env);
    
    client.mint(&account1, &100, &minter);
    client.mint(&account2, &200, &minter);
    client.freeze_account(&account1);
    client.freeze_account(&account2);
    
    let supply_before = client.total_supply();
    
    client.clawback(&account1, &50);
    assert_eq!(client.balance(&account1), 50);
    assert_eq!(client.balance(&account2), 200);
    assert_eq!(client.total_supply(), supply_before - 50);
    
    client.clawback(&account2, &100);
    assert_eq!(client.balance(&account1), 50);
    assert_eq!(client.balance(&account2), 100);
    assert_eq!(client.total_supply(), supply_before - 150);
}

#[test]
fn test_clawback_then_unfreeze_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let account = Address::generate(&env);
    
    client.mint(&account, &100, &minter);
    client.freeze_account(&account);
    client.clawback(&account, &60);
    
    assert_eq!(client.balance(&account), 40);
    
    // Unfreeze and verify account can transact again
    client.unfreeze_account(&account);
    client.set_transfer_locked(&admin, &false);
    
    let recipient = Address::generate(&env);
    client.transfer(&account, &recipient, &20);
    
    assert_eq!(client.balance(&account), 20);
    assert_eq!(client.balance(&recipient), 20);
}

#[test]
fn test_clawback_conserves_supply_invariant() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    
    let account1 = Address::generate(&env);
    let account2 = Address::generate(&env);
    let account3 = Address::generate(&env);
    
    client.mint(&account1, &100, &minter);
    client.mint(&account2, &200, &minter);
    client.mint(&account3, &300, &minter);
    
    let initial_supply = client.total_supply();
    assert_eq!(initial_supply, 600);
    
    client.freeze_account(&account2);
    client.clawback(&account2, &150);
    
    // Supply should decrease by clawback amount
    assert_eq!(client.total_supply(), initial_supply - 150);
    assert_eq!(
        client.balance(&account1) + client.balance(&account2) + client.balance(&account3),
        client.total_supply()
    );
}

#[test]
fn test_transfer_locked_with_sufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &10000, &minter);
    assert_eq!(client.balance(&user), 10000);

    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    assert_eq!(client.balance(&user), 10000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.set_transfer_locked(&admin, &false);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.mint(&sender, &50, &minter);

    let result = client.try_transfer(&sender, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientBalance)));
    assert_eq!(client.balance(&sender), 50);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &250);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250);
}

#[test]
fn test_approve_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let amount = 500;
    let expiration = env.ledger().sequence() + 100;

    client.approve(&admin, &spender, &amount, &expiration);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (Symbol::new(&env, "approve"), admin.clone(), spender.clone()).into_val(&env)
    );

    let event_data: (i128, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, amount);
    assert_eq!(event_data.1, expiration);
}

#[test]
fn test_mint_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let amount = 5000;

    client.mint(&recipient, &amount, &minter);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (Symbol::new(&env, "mint"), recipient.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, amount);
}

#[test]
fn test_mint_authorization_and_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.mint(&recipient, &100, &admin);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.total_supply(), 100);

    client.mint(&recipient, &50, &minter);
    assert_eq!(client.balance(&recipient), 150);
    assert_eq!(client.total_supply(), 150);

    let unauthorized = client.try_mint(&recipient, &25, &stranger);
    assert_eq!(unauthorized, Err(Ok(crate::errors::Error::Unauthorized)));

    let invalid_amount = client.try_mint(&recipient, &0, &minter);
    assert_eq!(invalid_amount, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.balance(&recipient), 150);
    assert_eq!(client.total_supply(), 150);
}

#[test]
fn test_burn_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let burn_amount = 300;
    client.burn(&admin, &burn_amount);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_burn_insufficient_balance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &100, &minter);

    let result = client.try_burn(&admin, &200);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientBalance)));
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
}

#[test]
fn test_transfer_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &200);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 200);
}

#[test]
fn test_burn_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let burn_amount = 150;
    client.burn_from(&spender, &admin, &burn_amount);

    let events = env.events().all();
    let event = events.events().last().unwrap();

    let (contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_burn_updates_balance_and_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    client.burn(&admin, &300);
    assert_eq!(client.balance(&admin), 700);
    assert_eq!(client.total_supply(), 700);
}

#[test]
fn test_burn_from_updates_balance_allowance_and_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    client.burn_from(&spender, &admin, &150);
    assert_eq!(client.balance(&admin), 850);
    assert_eq!(client.total_supply(), 850);
    assert_eq!(client.allowance(&admin, &spender), 350);
}

#[test]
fn test_burn_from_allowance_and_balance_checks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &100, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &50, &(current_ledger + 100));
    let allowance_fail = client.try_burn_from(&spender, &admin, &60);
    assert_eq!(
        allowance_fail,
        Err(Ok(crate::errors::Error::InsufficientAllowance))
    );

    client.approve(&admin, &spender, &200, &(current_ledger + 100));
    let balance_fail = client.try_burn_from(&spender, &admin, &150);
    assert_eq!(
        balance_fail,
        Err(Ok(crate::errors::Error::InsufficientBalance))
    );
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.allowance(&admin, &spender), 200);
}

#[test]
fn test_no_transfer_event_on_locked_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let events_before = env.events().all().events().len();

    let result = client.try_transfer(&user, &recipient, &100);
    assert!(result.is_err());

    let events_after = env.events().all();

    for i in events_before..events_after.events().len() {
        let event = events_after.events().get(i).unwrap();
        let (_addr, topics, _data) = parse_event(&env, event);
        if let Some(first_topic) = topics.get(0) {
            let symbol: Symbol = first_topic.try_into_val(&env).unwrap();
            assert_ne!(symbol, Symbol::new(&env, "transfer"));
        }
    }
}

#[test]
fn test_multiple_events_in_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.set_transfer_locked(&admin, &false);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.mint(&user1, &1000, &minter);
    client.transfer(&user1, &user2, &300);
    client.burn(&user2, &100);

    let events = env.events().all();
    let event_count = events.events().len();

    if event_count >= 3 {
        // Find and verify mint event (3rd from last)
        let mint_event = events.events().iter().rev().nth(2).unwrap();
        let (addr1, topics1, _data1) = parse_event(&env, mint_event);
        assert_eq!(
            topics1,
            (Symbol::new(&env, "mint"), user1.clone()).into_val(&env)
        );

        // Find and verify transfer event (2nd from last)
        let transfer_event = events.events().iter().rev().nth(1).unwrap();
        let (addr2, topics2, _data2) = parse_event(&env, transfer_event);
        assert_eq!(
            topics2,
            (Symbol::new(&env, "transfer"), user1.clone(), user2.clone()).into_val(&env)
        );

        // Find and verify burn event (last)
        let burn_event = events.events().last().unwrap();
        let (addr3, topics3, _data3) = parse_event(&env, burn_event);
        assert_eq!(
            topics3,
            (Symbol::new(&env, "burn"), user2.clone()).into_val(&env)
        );
    } else {
        assert!(event_count > 0, "Expected at least one event");
    }
}

// ========== Allowance Expiration Boundary Tests ==========

#[test]
fn test_approve_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();
    client.approve(&admin, &spender, &500, &current_ledger);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_approve_expiration_below_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert!(result.is_err());
}

#[test]
fn test_approve_expiration_zero_amount_allows_past() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
    assert_eq!(client.allowance(&admin, &spender), 500);

    client.approve(&admin, &spender, &0, &(current_ledger - 5));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_transfer_from_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &current_ledger);
    client.transfer_from(&spender, &admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.balance(&recipient), 200);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_transfer_from_expiration_one_below_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &initial_ledger);
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    let result = client.try_transfer_from(&spender, &admin, &recipient, &200);
    assert!(result.is_err());

    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
    client.transfer_from(&spender, &admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.balance(&recipient), 200);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_transfer_from_insufficient_allowance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &50, &expiration);

    let result = client.try_transfer_from(&spender, &admin, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::InsufficientAllowance)));
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.allowance(&admin, &spender), 50);
}

#[test]
fn test_burn_from_expiration_at_current_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &current_ledger);
    client.burn_from(&spender, &admin, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.total_supply(), 800);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_burn_from_expiration_one_below_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &initial_ledger);
    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    let result = client.try_burn_from(&spender, &admin, &200);
    assert!(result.is_err());

    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.total_supply(), 1000);
}

#[test]
fn test_burn_from_expiration_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.mint(&admin, &1000, &minter);

    let spender = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(current_ledger + 100));
    client.burn_from(&spender, &admin, &200);

    assert_eq!(client.balance(&admin), 800);
    assert_eq!(client.total_supply(), 800);
    assert_eq!(client.allowance(&admin, &spender), 300);
}

#[test]
fn test_allowance_returns_zero_when_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &initial_ledger);
    assert_eq!(client.allowance(&admin, &spender), 500);

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_allowance_boundary_multiple_ledgers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();
    let expiration = initial_ledger + 5;

    client.approve(&admin, &spender, &500, &expiration);

    for i in 0..=5 {
        env.ledger()
            .with_mut(|li| li.sequence_number = initial_ledger + i);
        let expected = if i <= 5 { 500 } else { 0 };
        assert_eq!(client.allowance(&admin, &spender), expected);
    }

    env.ledger()
        .with_mut(|li| li.sequence_number = expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_update_expiration_extends() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(initial_ledger + 2));

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 1);

    client.approve(&admin, &spender, &600, &(initial_ledger + 10));

    env.ledger()
        .with_mut(|li| li.sequence_number = initial_ledger + 3);

    assert_eq!(client.allowance(&admin, &spender), 600);
}

#[test]
fn test_approve_update_expiration_shortens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let initial_ledger = env.ledger().sequence();

    client.approve(&admin, &spender, &500, &(initial_ledger + 100));

    let new_expiration = initial_ledger + 2;
    client.approve(&admin, &spender, &600, &new_expiration);

    env.ledger()
        .with_mut(|li| li.sequence_number = new_expiration);
    assert_eq!(client.allowance(&admin, &spender), 600);

    env.ledger()
        .with_mut(|li| li.sequence_number = new_expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

// ========== Negative Amount Tests ==========

#[test]
fn test_approve_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    let result = client.try_approve(&admin, &spender, &(-100i128), &expiration);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_zero_amount_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    client.approve(&admin, &spender, &500, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 500);

    client.approve(&admin, &spender, &0, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_approve_positive_amount_invalid_expiration_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    env.ledger().with_mut(|li| li.sequence_number = 10);
    let current_ledger = env.ledger().sequence();

    let result = client.try_approve(&admin, &spender, &500, &(current_ledger - 1));
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));
    assert_eq!(client.allowance(&admin, &spender), 0);
}

// ========== Issue #106: Nonce Tests ==========

#[test]
fn test_get_nonce_initial_is_zero() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    let nonce = client.get_nonce(&admin);
    assert_eq!(nonce, 0);
}

#[test]
fn test_get_nonce_returns_zero_for_unknown_account() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    let unknown = Address::generate(&env);
    let nonce = client.get_nonce(&unknown);
    assert_eq!(nonce, 0);
}

#[test]
fn test_get_nonce_emits_event() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    let _nonce = client.get_nonce(&admin);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);

    assert_eq!(topics, (Symbol::new(&env, "nonce_queried"),).into_val(&env));

    let (account, emitted_nonce): (Address, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(account, admin);
    assert_eq!(emitted_nonce, 0);
}

// ========== Issue #113: Fee Deduction Tests ==========

#[test]
fn test_get_fee_bps_initial_zero() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    client.set_fee_bps(&admin, &250); // 2.5%
    assert_eq!(client.get_fee_bps(), 250);
}

#[test]
fn test_set_fee_bps_unauthorized_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let stranger = Address::generate(&env);
    let result = client.try_set_fee_bps(&stranger, &250);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_invalid_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let result = client.try_set_fee_bps(&admin, &10_001);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidFeeBps)));
    assert_eq!(client.get_fee_bps(), 0);

    let result = client.try_set_fee_bps(&admin, &-1);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidFeeBps)));
    assert_eq!(client.get_fee_bps(), 0);
}

#[test]
fn test_set_fee_bps_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    client.set_fee_bps(&admin, &250);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    // redundant destructuring removed

    assert_eq!(topics, (Symbol::new(&env, "fee_updated"),).into_val(&env));

    let event_data: (i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (0, 250));
}

#[test]
fn test_transfer_with_fee_deducts_from_sender() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 10% (1000 bps)
    client.set_fee_bps(&admin, &1000);

    // Unlock transfers
    client.set_transfer_locked(&admin, &false);

    // Mint sufficient balance (amount + fee needed)
    let user = Address::generate(&env);
    client.mint(&user, &5000, &minter);
    assert_eq!(client.balance(&user), 5000);

    // Transfer 1000: fee = 1000 * 1000 / 10000 = 100
    let recipient = Address::generate(&env);
    client.transfer(&user, &recipient, &1000);

    assert_eq!(client.balance(&user), 5000 - 1000 - 100); // 3900
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.balance(&admin), 100); // admin received fee
}

#[test]
fn test_transfer_fee_insufficient_for_total_debit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 10% (1000 bps)
    client.set_fee_bps(&admin, &1000);
    client.set_transfer_locked(&admin, &false);

    // Mint exactly 1050 to a user (amount=1000 + fee=100 = 1100, but only 1050 available)
    let user = Address::generate(&env);
    client.mint(&user, &1050, &minter);

    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &1000);
    // 1000 * 1000/10000 = 100, total_debit = 1100 > 1050
    // Balance >= amount (1050 >= 1000), but balance < total_debit
    assert_eq!(
        result,
        Err(Ok(crate::errors::Error::InsufficientBalanceForFee))
    );
    assert_eq!(client.balance(&user), 1050);
}

#[test]
fn test_transfer_zero_fee_no_deduction() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Fee is 0 by default
    assert_eq!(client.get_fee_bps(), 0);

    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    client.set_transfer_locked(&admin, &false);

    let recipient = Address::generate(&env);
    client.transfer(&user, &recipient, &500);

    assert_eq!(client.balance(&user), 500);
    assert_eq!(client.balance(&recipient), 500);
    assert_eq!(client.balance(&admin), 0); // no fee collected
}

#[test]
fn test_transfer_from_with_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Set fee to 5% (500 bps)
    client.set_fee_bps(&admin, &500);
    client.set_transfer_locked(&admin, &false);

    // Mint 2000 to admin, approve spender
    client.mint(&admin, &2000, &minter);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &1000, &expiration);

    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &1000);

    // fee = 1000 * 500 / 10000 = 50
    // Admin debited: 1000 (amount) + 50 (fee) = 1050
    // Admin credited: 50 (fee goes to admin)
    // Net admin: 2000 - 1050 + 50 = 1000
    assert_eq!(client.balance(&recipient), 1000);
    assert_eq!(client.balance(&admin), 1000);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_sub_asset_decimals_can_be_updated_within_supported_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    client.set_decimals(&18);
    let events = env.events().all();
    assert_eq!(client.decimals(), 18);

    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "decimals_updated"),).into_val(&env)
    );
    let event_data: (u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, (7, 18));
}

#[test]
fn test_sub_asset_decimals_reject_unsupported_precision() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let result = client.try_set_decimals(&19);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidDecimals)));
    assert_eq!(client.decimals(), 7);
}

#[test]
fn test_initialize_rejects_unsupported_precision() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-001");
    let symbol = SorobanString::from_str(&env, "INV001");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &name, &symbol, &19, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidDecimals)));
}

#[test]
fn test_balance_batch_preserves_order_and_includes_zero_balances() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);
    let first = Address::generate(&env);
    let empty = Address::generate(&env);
    let second = Address::generate(&env);
    client.mint(&first, &25, &minter);
    client.mint(&second, &50, &minter);

    let mut ids = Vec::new(&env);
    ids.push_back(first.clone());
    ids.push_back(empty.clone());
    ids.push_back(second.clone());
    ids.push_back(first);

    let balances = client.balance_batch(&ids);
    assert_eq!(balances, soroban_sdk::vec![&env, 25i128, 0, 50, 25]);
}

#[test]
fn test_balance_batch_requires_initialization() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let ids = soroban_sdk::vec![&env, Address::generate(&env)];

    assert_eq!(
        client.try_balance_batch(&ids),
        Err(Ok(crate::errors::Error::NotInit))
    );
}

#[test]
fn test_revoke_approval_clears_allowance_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    client.revoke_approval(&admin, &spender);
    let events = env.events().all();
    assert_eq!(client.allowance(&admin, &spender), 0);

    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "approval_revoked"), admin, spender).into_val(&env)
    );
    let event_data: () = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, ());
}

#[test]
fn test_revoke_approval_requires_from_authorization() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);

    assert!(client.try_revoke_approval(&admin, &spender).is_err());
}

#[test]
fn test_pause_blocks_burn_and_burn_from() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.mint(&admin, &100, &minter);
    client.approve(&admin, &spender, &100, &expiration);
    client.set_paused(&true);

    assert_eq!(
        client.try_burn(&admin, &10),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_burn_from(&spender, &admin, &10),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(client.balance(&admin), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.allowance(&admin, &spender), 100);
}

#[test]
fn test_initialize_rejects_empty_name() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let empty_name = SorobanString::from_str(&env, "");
    let symbol = SorobanString::from_str(&env, "INV001");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &empty_name, &symbol, &7u32, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidMetadata)));
}

#[test]
fn test_initialize_rejects_empty_symbol() {
    let env = Env::default();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-001");
    let empty_symbol = SorobanString::from_str(&env, "");
    let invoice_id = Symbol::new(&env, "inv_001");

    let result = client.try_initialize(&admin, &name, &empty_symbol, &7u32, &invoice_id, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidMetadata)));
}

#[test]
fn test_extend_allowance_updates_expiration_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let new_expiration = expiration + 200;
    client.extend_allowance(&admin, &spender, &new_expiration);

    // Amount is unchanged; only the expiration ledger moved forward.
    assert_eq!(client.allowance(&admin, &spender), 500);

    // Ledger advances past the original expiration but before the extended one:
    // the allowance must still be usable.
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_extend_allowance_requires_later_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    let result = client.try_extend_allowance(&admin, &spender, &expiration);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));

    let result = client.try_extend_allowance(&admin, &spender, &(expiration - 1));
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidExpiration)));
}

#[test]
fn test_extend_allowance_fails_without_existing_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);
    let spender = Address::generate(&env);

    let result = client.try_extend_allowance(&admin, &spender, &(env.ledger().sequence() + 100));
    assert_eq!(result, Err(Ok(crate::errors::Error::AllowanceNotFound)));
}

#[test]
fn test_extend_allowance_fails_after_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let spender = Address::generate(&env);

    client.mint(&admin, &1000, &minter);
    let current = env.ledger().sequence();
    let expiration = current + 10;
    client.approve(&admin, &spender, &500, &expiration);

    // Move past expiration before attempting to extend.
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);
    let result = client.try_extend_allowance(&admin, &spender, &(expiration + 100));
    assert_eq!(result, Err(Ok(crate::errors::Error::AllowanceExpired)));
}

// ========== Storage Key Serialization Tests (Issue #161) ==========

/// Helper: assert a StorageKey roundtrips losslessly through Val.
fn assert_key_roundtrip(env: &Env, key: StorageKey) {
    let val: Val = key.clone().into_val(env);
    let back: StorageKey = StorageKey::try_from_val(env, &val).expect("roundtrip failed");
    assert_eq!(key, back, "StorageKey roundtrip equality");
}

/// Each StorageKey variant serializes and deserializes without data loss.
#[test]
fn test_storage_key_serialization_roundtrip_metadata() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::Metadata);
}

#[test]
fn test_storage_key_serialization_roundtrip_total_supply() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::TotalSupply);
}

#[test]
fn test_storage_key_serialization_roundtrip_fee_bps() {
    let env = Env::default();
    assert_key_roundtrip(&env, StorageKey::FeeBps);
}

#[test]
fn test_storage_key_serialization_roundtrip_balance() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Balance(addr));
}

#[test]
fn test_storage_key_serialization_roundtrip_allowance() {
    let env = Env::default();
    let from = Address::generate(&env);
    let spender = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Allowance(from, spender));
}

#[test]
fn test_storage_key_serialization_roundtrip_role_admin() {
    let env = Env::default();
    let role = Symbol::new(&env, "admin");
    assert_key_roundtrip(&env, StorageKey::RoleAdmin(role));
}

#[test]
fn test_storage_key_serialization_roundtrip_role_grant() {
    let env = Env::default();
    let role = Symbol::new(&env, "minter");
    let account = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::RoleGrant(role, account));
}

#[test]
fn test_storage_key_serialization_roundtrip_nonce() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Nonce(addr));
}

#[test]
fn test_storage_key_serialization_roundtrip_history() {
    let env = Env::default();
    let addr = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::History(addr));
}

#[test]
fn test_storage_key_serialization_roundtrip_frozen() {
    let env = Env::default();
    let account = Address::generate(&env);
    assert_key_roundtrip(&env, StorageKey::Frozen(account));
}

/// All StorageKey variants produce distinct serialized values (no collisions).
#[test]
fn test_storage_key_uniqueness_all_variants_distinct() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let role_admin = Symbol::new(&env, "admin");

    let keys = [
        StorageKey::Metadata,
        StorageKey::TotalSupply,
        StorageKey::FeeBps,
        StorageKey::Balance(addr_a.clone()),
        StorageKey::Allowance(addr_a.clone(), addr_b.clone()),
        StorageKey::RoleAdmin(role_admin.clone()),
        StorageKey::RoleGrant(role_admin.clone(), addr_a.clone()),
        StorageKey::Nonce(addr_a.clone()),
        StorageKey::History(addr_a.clone()),
        StorageKey::Frozen(addr_a.clone()),
    ];

    let n = keys.len();
    for i in 0..n {
        for j in (i + 1)..n {
            assert_ne!(
                keys[i], keys[j],
                "StorageKey variants at indices {i} and {j} must be distinct"
            );
        }
    }
}

/// Same variant with different data produces different keys.
#[test]
fn test_storage_key_same_variant_different_data_distinct() {
    let env = Env::default();
    let addr = Address::generate(&env);
    let other_addr = Address::generate(&env);

    // Balance with different addresses
    assert_ne!(
        StorageKey::Balance(addr.clone()),
        StorageKey::Balance(other_addr.clone()),
        "Balance keys with different addresses must differ"
    );

    // Nonce with different addresses
    assert_ne!(
        StorageKey::Nonce(addr.clone()),
        StorageKey::Nonce(other_addr.clone()),
        "Nonce keys with different addresses must differ"
    );

    // History with different addresses
    assert_ne!(
        StorageKey::History(addr.clone()),
        StorageKey::History(other_addr.clone()),
        "History keys with different addresses must differ"
    );

    // Frozen with different addresses
    assert_ne!(
        StorageKey::Frozen(addr.clone()),
        StorageKey::Frozen(other_addr.clone()),
        "Frozen keys with different addresses must differ"
    );

    // Allowance with different pairs
    let third = Address::generate(&env);
    assert_ne!(
        StorageKey::Allowance(addr.clone(), other_addr.clone()),
        StorageKey::Allowance(other_addr.clone(), addr.clone()),
        "Allowance (A,B) must differ from (B,A)"
    );
    assert_ne!(
        StorageKey::Allowance(addr.clone(), other_addr.clone()),
        StorageKey::Allowance(addr.clone(), third.clone()),
        "Allowance with different spender must differ"
    );
    assert_ne!(
        StorageKey::Allowance(other_addr.clone(), addr.clone()),
        StorageKey::Allowance(addr.clone(), third.clone()),
        "Allowance with different from must differ"
    );

    // RoleAdmin with different symbols
    assert_ne!(
        StorageKey::RoleAdmin(Symbol::new(&env, "admin")),
        StorageKey::RoleAdmin(Symbol::new(&env, "minter")),
        "RoleAdmin keys with different roles must differ"
    );

    // RoleGrant with different roles or accounts
    assert_ne!(
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), addr.clone()),
        StorageKey::RoleGrant(Symbol::new(&env, "admin"), addr.clone()),
        "RoleGrant keys with different roles must differ"
    );
    assert_ne!(
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), addr.clone()),
        StorageKey::RoleGrant(Symbol::new(&env, "pauser"), other_addr.clone()),
        "RoleGrant keys with different accounts must differ"
    );
}

/// Instance-storage keys are distinct from persistent-storage keys.
/// This test verifies that the XDR serialization preserves the variant
/// discriminant so instance keys never collide with persistent keys.
#[test]
fn test_storage_key_instance_vs_persistent_distinct() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let instance_keys = [
        StorageKey::Metadata,
        StorageKey::TotalSupply,
        StorageKey::FeeBps,
        StorageKey::RoleAdmin(Symbol::new(&env, "role")),
        StorageKey::RoleGrant(Symbol::new(&env, "role"), addr.clone()),
    ];

    let persistent_keys = [
        StorageKey::Balance(addr.clone()),
        StorageKey::Allowance(addr.clone(), Address::generate(&env)),
        StorageKey::Nonce(addr.clone()),
        StorageKey::History(addr.clone()),
    ];

    for ik in &instance_keys {
        for pk in &persistent_keys {
            assert_ne!(
                ik, pk,
                "Instance key {:?} must not collide with persistent key {:?}",
                ik, pk
            );
        }
    }
}

/// Deterministic serialization: the same key produces the same serialized value.
/// Verified through StorageKey equality (which derives PartialEq) since XDR
/// serialization is deterministic for the same key data.
#[test]
fn test_storage_key_deterministic_serialization() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    // Roundtrip: StorageKey -> Val -> StorageKey preserves identity
    let key = StorageKey::Balance(addr_a.clone());
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic Balance roundtrip");

    let key = StorageKey::Allowance(addr_a.clone(), addr_b.clone());
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic Allowance roundtrip");

    let key = StorageKey::RoleAdmin(Symbol::new(&env, "admin"));
    let val: Val = key.clone().into_val(&env);
    let back = StorageKey::try_from_val(&env, &val).expect("roundtrip");
    assert_eq!(key, back, "Deterministic RoleAdmin roundtrip");

    // Identically constructed keys are equal (deterministic construction)
    assert_eq!(
        StorageKey::Balance(addr_a.clone()),
        StorageKey::Balance(addr_a),
        "Identical Balance keys must be equal"
    );
}

/// Edge case: empty Symbol in RoleAdmin and RoleGrant.
#[test]
fn test_storage_key_empty_symbol_serialization() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let empty_role = Symbol::new(&env, "");
    let key_admin = StorageKey::RoleAdmin(empty_role.clone());
    let key_grant = StorageKey::RoleGrant(empty_role.clone(), addr);

    assert_key_roundtrip(&env, key_admin);
    assert_key_roundtrip(&env, key_grant);
}

/// Edge case: multi-byte Symbol values in StorageKey.
#[test]
fn test_storage_key_multibyte_symbol_serialization() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let long_role = Symbol::new(&env, "a_really_long_role_name_12345");
    let key = StorageKey::RoleGrant(long_role.clone(), addr.clone());
    assert_key_roundtrip(&env, key);

    let key = StorageKey::RoleAdmin(long_role);
    assert_key_roundtrip(&env, key);
}

/// StorageKey::Balance(addr) produces the same key value when cloned.
#[test]
fn test_storage_key_balance_clone_produces_same_key() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let key1 = StorageKey::Balance(addr.clone());
    let key2 = StorageKey::Balance(addr);
    assert_eq!(
        key1, key2,
        "Cloned Balance keys must produce identical values"
    );
}

/// Many different addresses produce distinct Balance storage keys.
#[test]
fn test_storage_key_many_balance_addresses_distinct() {
    let env = Env::default();
    let mut seen: Vec<StorageKey> = Vec::new(&env);

    for _ in 0..20 {
        let addr = Address::generate(&env);
        let key = StorageKey::Balance(addr);

        for existing in seen.iter() {
            assert_ne!(key, existing, "Generated Balance keys must be unique");
        }
        seen.push_back(key);
    }
}

/// Allowance key uniqueness across many address pairs.
#[test]
fn test_storage_key_many_allowance_pairs_distinct() {
    let env = Env::default();
    let mut seen: Vec<StorageKey> = Vec::new(&env);

    for _ in 0..15 {
        let from = Address::generate(&env);
        let spender = Address::generate(&env);
        let key = StorageKey::Allowance(from, spender);

        for existing in seen.iter() {
            assert_ne!(key, existing, "Generated Allowance keys must be unique");
        }
        seen.push_back(key);
    }

    // Also verify that the same pair repeated gives the same key (determinism)
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let k1 = StorageKey::Allowance(addr_a.clone(), addr_b.clone());
    let k2 = StorageKey::Allowance(addr_a, addr_b);
    assert_eq!(k1, k2, "Same Allowance pair must produce identical key");
}

/// Full lifecycle: write data via storage key, read it back, verify integrity.
#[test]
fn test_storage_key_total_supply_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Test");
    let symbol = SorobanString::from_str(&env, "TST");
    let invoice_id = Symbol::new(&env, "inv");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);

    // Initial total supply should be 0
    assert_eq!(client.total_supply(), 0);

    // Mint and verify total supply increases
    let user = Address::generate(&env);
    client.mint(&user, &5000, &minter);
    assert_eq!(client.total_supply(), 5000);

    // Burn and verify total supply decreases
    client.burn(&user, &2000);
    assert_eq!(client.total_supply(), 3000);
}

/// Verify StorageKey::Metadata lifecycle through full initialize + read flow.
#[test]
fn test_storage_key_metadata_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);

    // Verify metadata is persisted and accessible
    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert!(client.transfer_locked());

    // Update decimals - verify metadata mutation works via storage key
    client.set_decimals(&12);
    assert_eq!(client.decimals(), 12);
}

/// Verify StorageKey::Balance lifecycle: mint, transfer, balance queries.
#[test]
fn test_storage_key_balance_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    client.mint(&alice, &1000, &minter);
    assert_eq!(client.balance(&alice), 1000);

    // Transfer from alice to bob
    client.transfer(&alice, &bob, &400);
    assert_eq!(client.balance(&alice), 600);
    assert_eq!(client.balance(&bob), 400);

    // Verify uninitialized address returns 0
    let charlie = Address::generate(&env);
    assert_eq!(client.balance(&charlie), 0);
}

/// Verify StorageKey::Allowance lifecycle: approve, transfer_from, allowance queries.
#[test]
fn test_storage_key_allowance_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);
    client.mint(&admin, &2000, &minter);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;

    // Approve
    client.approve(&admin, &spender, &1000, &expiration);
    assert_eq!(client.allowance(&admin, &spender), 1000);

    // Transfer from reduces allowance
    client.transfer_from(&spender, &admin, &recipient, &300);
    assert_eq!(client.allowance(&admin, &spender), 700);
    assert_eq!(client.balance(&recipient), 300);

    // Revoke approval
    client.revoke_approval(&admin, &spender);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

/// Verify StorageKey::Nonce lifecycle.
#[test]
fn test_storage_key_nonce_storage_lifecycle() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    // Initial nonce is 0
    assert_eq!(client.get_nonce(&admin), 0);

    // Querying nonce doesn't change it
    assert_eq!(client.get_nonce(&admin), 0);

    // Unknown account also returns 0
    let unknown = Address::generate(&env);
    assert_eq!(client.get_nonce(&unknown), 0);
}

/// Verify StorageKey::History lifecycle.
#[test]
fn test_storage_key_history_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice (no history entry for mint)
    client.mint(&alice, &1000, &minter);

    // Transfer from alice to bob should create a history record
    client.transfer(&alice, &bob, &400);

    // Check history for alice
    let alice_history = client.get_token_history(&alice);
    assert!(
        alice_history.len() > 0,
        "Alice should have history after transferring out"
    );

    // Check history for bob
    let bob_history = client.get_token_history(&bob);
    assert!(
        bob_history.len() > 0,
        "Bob should have history after receiving tokens"
    );
}

/// Verify StorageKey::FeeBps lifecycle.
#[test]
fn test_storage_key_fee_bps_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);

    // Default fee is 0
    assert_eq!(client.get_fee_bps(), 0);

    // Set fee
    client.set_fee_bps(&admin, &500);
    assert_eq!(client.get_fee_bps(), 500);

    // Update fee
    client.set_fee_bps(&admin, &1000);
    assert_eq!(client.get_fee_bps(), 1000);
}

/// Verify RoleAdmin and RoleGrant storage key lifecycles.
#[test]
fn test_storage_key_role_storage_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);

    let account = Address::generate(&env);

    // Use a pre-configured role (admin) since `set_role_admin` requires the caller
    // to be the current admin of that role, and new roles have no admin.
    let role = Symbol::new(&env, "admin");

    // Initially the admin is the role admin (set during initialize)
    let role_admin: Address = client.get_role_admin(&role);
    assert_eq!(role_admin, admin.clone());

    // Change the role admin to someone else
    let new_admin = Address::generate(&env);
    client.set_role_admin(&admin, &role, &new_admin);
    let updated_admin: Address = client.get_role_admin(&role);
    assert_eq!(updated_admin, new_admin.clone());

    // Initially role not granted for a new account
    assert!(!client.has_role(&role, &account));

    // Grant role
    client.grant_role(&new_admin, &role, &account);
    assert!(client.has_role(&role, &account));

    // Revoke role
    client.revoke_role(&new_admin, &role, &account);
    assert!(!client.has_role(&role, &account));
}

/// Unconfigured roles return RoleNotGranted when queried.
#[test]
fn test_storage_key_unconfigured_role_returns_error() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    let custom_role = Symbol::new(&env, "nonexistent_role");
    let result = client.try_get_role_admin(&custom_role);
    assert_eq!(result, Err(Ok(crate::errors::Error::RoleNotGranted)));
}

/// Verify that two different roles with the same account produce distinct keys.
#[test]
fn test_storage_key_role_grant_cross_role_distinct() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _minter) = setup_token(&env);
    let account = Address::generate(&env);

    let role_a = Symbol::new(&env, "admin");
    let role_b = Symbol::new(&env, "minter");

    client.grant_role(&admin, &role_a, &account);
    client.grant_role(&admin, &role_b, &account);

    assert!(client.has_role(&role_a, &account));
    assert!(client.has_role(&role_b, &account));

    // Revoke only role_a
    client.revoke_role(&admin, &role_a, &account);
    assert!(!client.has_role(&role_a, &account));
    assert!(
        client.has_role(&role_b, &account),
        "Role B must still be granted after revoking role A"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #147: Comprehensive Unit Tests for Invoice Token Pause/Unpause State Switch
// ══════════════════════════════════════════════════════════════════════════════

/// Verify the full lifecycle of pausing and unpausing the invoice token contract.
/// All state-mutating operations (mint, mint_batch, transfer, transfer_from, approve,
/// extend_allowance, revoke_approval, burn, burn_from) must be blocked when paused,
/// and must immediately succeed again when unpaused.
#[test]
fn test_pause_unpause_state_switch_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let holder_a = Address::generate(&env);
    let holder_b = Address::generate(&env);
    let spender = Address::generate(&env);

    // Initial setup: mint to holder_a and holder_b, unlock transfers
    client.mint(&holder_a, &1000, &minter);
    client.mint(&holder_b, &500, &minter);
    client.set_transfer_locked(&admin, &false);

    let expiration = env.ledger().sequence() + 100;
    client.approve(&holder_a, &spender, &400, &expiration);

    // Initially unpaused
    assert_eq!(client.paused(), false);

    // 1. Pause contract
    client.set_paused(&true);
    assert_eq!(client.paused(), true);

    // Verify all operations fail with Error::Paused
    assert_eq!(
        client.try_transfer(&holder_a, &holder_b, &50),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_transfer_from(&spender, &holder_a, &holder_b, &50),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_approve(&holder_a, &spender, &600, &(expiration + 50)),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_extend_allowance(&holder_a, &spender, &(expiration + 50)),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_revoke_approval(&holder_a, &spender),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_mint(&holder_a, &200, &minter),
        Err(Ok(crate::errors::Error::Paused))
    );
    let recipients = soroban_sdk::vec![&env, holder_a.clone(), holder_b.clone()];
    let amounts = soroban_sdk::vec![&env, 100i128, 100i128];
    assert_eq!(
        client.try_mint_batch(&recipients, &amounts, &minter),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_burn(&holder_a, &50),
        Err(Ok(crate::errors::Error::Paused))
    );
    assert_eq!(
        client.try_burn_from(&spender, &holder_a, &50),
        Err(Ok(crate::errors::Error::Paused))
    );

    // Verify state was not mutated during paused attempts
    assert_eq!(client.balance(&holder_a), 1000);
    assert_eq!(client.balance(&holder_b), 500);
    assert_eq!(client.total_supply(), 1500);
    assert_eq!(client.allowance(&holder_a, &spender), 400);

    // 2. Unpause contract
    client.set_paused(&false);
    assert_eq!(client.paused(), false);

    // Verify operations succeed after unpausing
    client.transfer(&holder_a, &holder_b, &100);
    assert_eq!(client.balance(&holder_a), 900);
    assert_eq!(client.balance(&holder_b), 600);

    client.transfer_from(&spender, &holder_a, &holder_b, &100);
    assert_eq!(client.balance(&holder_a), 800);
    assert_eq!(client.balance(&holder_b), 700);
    assert_eq!(client.allowance(&holder_a, &spender), 300);

    client.extend_allowance(&holder_a, &spender, &(expiration + 50));
    client.mint(&holder_a, &200, &minter);
    assert_eq!(client.balance(&holder_a), 1000);
    assert_eq!(client.total_supply(), 1700);

    client.mint_batch(&recipients, &amounts, &minter);
    assert_eq!(client.balance(&holder_a), 1100);
    assert_eq!(client.balance(&holder_b), 800);
    assert_eq!(client.total_supply(), 1900);

    client.burn(&holder_a, &100);
    assert_eq!(client.balance(&holder_a), 1000);
    assert_eq!(client.total_supply(), 1800);

    client.burn_from(&spender, &holder_a, &100);
    assert_eq!(client.balance(&holder_a), 900);
    assert_eq!(client.allowance(&holder_a, &spender), 200);
    assert_eq!(client.total_supply(), 1700);

    client.revoke_approval(&holder_a, &spender);
    assert_eq!(client.allowance(&holder_a, &spender), 0);
}

/// Verify state persistence across multiple pause/unpause toggle cycles.
#[test]
fn test_pause_unpause_repeated_toggles_state_persistence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    let holder = Address::generate(&env);
    client.mint(&holder, &500, &minter);

    for _ in 0..3 {
        // Toggle on
        client.set_paused(&true);
        assert_eq!(client.paused(), true);
        assert_eq!(client.balance(&holder), 500);
        assert_eq!(client.total_supply(), 500);

        // Toggle off
        client.set_paused(&false);
        assert_eq!(client.paused(), false);
        assert_eq!(client.balance(&holder), 500);
        assert_eq!(client.total_supply(), 500);
    }
}

/// Verify setting pause state without admin auth fails.
#[test]
fn test_set_paused_non_admin_unauthorized() {
    let env = Env::default();
    let (client, _admin, _minter) = setup_token(&env);

    // Call without admin mock auth should fail
    let result = client.try_set_paused(&true);
    assert!(result.is_err());
}

/// Verify calling set_paused with the same value behaves idempotently.
#[test]
fn test_set_paused_idempotent_switch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    assert_eq!(client.paused(), false);

    // Unpause when already unpaused
    client.set_paused(&false);
    assert_eq!(client.paused(), false);

    // Pause
    client.set_paused(&true);
    assert_eq!(client.paused(), true);

    // Pause again when already paused
    client.set_paused(&true);
    assert_eq!(client.paused(), true);

    // Unpause
    client.set_paused(&false);
    assert_eq!(client.paused(), false);
}

/// Verify event emission when pausing and unpausing.
#[test]
fn test_pause_unpause_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    // 1. Pause
    client.set_paused(&true);
    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "paused_updated"),).into_val(&env)
    );
    let (old_val, new_val): (bool, bool) = data.try_into_val(&env).unwrap();
    assert_eq!(old_val, false);
    assert_eq!(new_val, true);

    // 2. Unpause
    client.set_paused(&false);
    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (_contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "paused_updated"),).into_val(&env)
    );
    let (old_val, new_val): (bool, bool) = data.try_into_val(&env).unwrap();
    assert_eq!(old_val, true);
    assert_eq!(new_val, false);
}

// ── Invoice mint uniqueness, authorization, and identifier encoding ──────────

/// First mint for an invoice token succeeds; re-initialize (second series) is
/// rejected with no further mint events or supply growth.
#[test]
fn test_duplicate_mint_series_rejected_for_same_invoice() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let recipient = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice INV-DUP");
    let symbol = SorobanString::from_str(&env, "INVDUP");
    let invoice_id = Symbol::new(&env, "inv_dup");

    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);

    let mint_amount: i128 = 1_000;
    client.mint(&recipient, &mint_amount, &minter);

    // Inspect mint event immediately (subsequent view calls can advance the
    // recorded event cursor in the test host).
    let events_after_mint = env.events().all();
    let mint_event = events_after_mint.events().last().unwrap();
    let (_addr, topics, data) = parse_event(&env, mint_event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "mint"), recipient.clone()).into_val(&env)
    );
    let emitted: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted, mint_amount);
    let mint_event_count = events_after_mint.events().len();
    assert_eq!(
        mint_event_count, 1,
        "exactly one mint event for the invoice"
    );

    assert_eq!(client.balance(&recipient), mint_amount);
    assert_eq!(client.total_supply(), mint_amount);
    assert_eq!(client.invoice_id(), invoice_id);

    // Repeated initialize with the same invoice id cannot create another series.
    let dup_init = client.try_initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    assert_eq!(dup_init, Err(Ok(crate::errors::Error::AlreadyInit)));

    assert_eq!(client.balance(&recipient), mint_amount);
    assert_eq!(client.total_supply(), mint_amount);
    assert_eq!(client.invoice_id(), invoice_id);
}

/// Authorized minter creates the expected token record; unauthorized and
/// invalid invoice metadata paths are rejected.
#[test]
fn test_authorized_mint_and_invalid_invoice_metadata() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let stranger = Address::generate(&env);
    let recipient = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice AUTH");
    let symbol = SorobanString::from_str(&env, "AUTH");
    let invoice_id = Symbol::new(&env, "inv_auth");

    // Empty name rejected before any token record exists.
    let empty_name = SorobanString::from_str(&env, "");
    assert_eq!(
        client.try_initialize(&admin, &empty_name, &symbol, &7u32, &invoice_id, &minter),
        Err(Ok(crate::errors::Error::InvalidMetadata))
    );

    // Empty symbol rejected.
    let empty_symbol = SorobanString::from_str(&env, "");
    assert_eq!(
        client.try_initialize(&admin, &name, &empty_symbol, &7u32, &invoice_id, &minter),
        Err(Ok(crate::errors::Error::InvalidMetadata))
    );

    // Valid initialize + authorized mint.
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    client.mint(&recipient, &250, &minter);
    assert_eq!(client.balance(&recipient), 250);
    assert_eq!(client.total_supply(), 250);
    assert_eq!(client.invoice_id(), invoice_id);
    assert_eq!(client.name(), name);
    assert_eq!(client.symbol(), symbol);

    // Unauthorized caller rejected; balances/supply unchanged.
    let unauthorized = client.try_mint(&recipient, &50, &stranger);
    assert_eq!(unauthorized, Err(Ok(crate::errors::Error::Unauthorized)));
    assert_eq!(client.balance(&recipient), 250);
    assert_eq!(client.total_supply(), 250);

    // Duplicate initialize (same invoice id on same contract) rejected.
    assert_eq!(
        client.try_initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter),
        Err(Ok(crate::errors::Error::AlreadyInit))
    );
    assert_eq!(client.invoice_id(), invoice_id);
    assert_eq!(client.total_supply(), 250);
}

/// Invoice identifiers and metadata round-trip through storage without loss;
/// stored metadata matches what initialize recorded for event consumers.
#[test]
fn test_invoice_identifier_and_metadata_encoding() {
    let env = Env::default();
    env.mock_all_auths();

    // Short identifier
    {
        let contract_id = env.register(InvoiceToken, ());
        let client = InvoiceTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        let short_id = Symbol::new(&env, "a");
        let name = SorobanString::from_str(&env, "Short Inv");
        let symbol = SorobanString::from_str(&env, "S");
        client.initialize(&admin, &name, &symbol, &7u32, &short_id, &minter);
        assert_eq!(client.invoice_id(), short_id);
        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);
        assert_eq!(client.decimals(), 7);
    }

    // Maximum-length Symbol (32 chars) — boundary for Soroban Symbol encoding
    {
        let contract_id = env.register(InvoiceToken, ());
        let client = InvoiceTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        let max_id = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz012345");
        let name = SorobanString::from_str(&env, "Max Length Invoice Identifier");
        let symbol = SorobanString::from_str(&env, "MAX32");
        client.initialize(&admin, &name, &symbol, &0u32, &max_id, &minter);
        assert_eq!(client.invoice_id(), max_id);
        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);
        assert_eq!(client.decimals(), 0);

        // Mint and confirm event payload encoding is independent of invoice_id
        // while invoice_id remains the stored metadata key for consumers.
        let recipient = Address::generate(&env);
        client.mint(&recipient, &42, &minter);
        let events = env.events().all();
        let event = events.events().last().unwrap();
        let (_addr, topics, data) = parse_event(&env, event);
        assert_eq!(
            topics,
            (Symbol::new(&env, "mint"), recipient.clone()).into_val(&env)
        );
        let emitted: i128 = data.try_into_val(&env).unwrap();
        assert_eq!(emitted, 42);
        assert_eq!(client.invoice_id(), max_id);
    }

    // Near-max boundary (31 chars) still round-trips
    {
        let contract_id = env.register(InvoiceToken, ());
        let client = InvoiceTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        let near_max = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz01234");
        let name = SorobanString::from_str(&env, "Near Max");
        let symbol = SorobanString::from_str(&env, "NMAX");
        client.initialize(&admin, &name, &symbol, &18u32, &near_max, &minter);
        assert_eq!(client.invoice_id(), near_max);
        assert_eq!(client.decimals(), 18);
    }
}

// ========== #387: Dedicated Error Branch Tests ==========

#[test]
fn test_mint_unauthorized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let stranger = Address::generate(&env);

    let result = client.try_mint(&recipient, &100, &stranger);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let result = client.try_mint(&recipient, &(-1i128), &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let result = client.try_mint(&recipient, &0, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_paused_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.set_paused(&true);
    let recipient = Address::generate(&env);
    let result = client.try_mint(&recipient, &100, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::Paused)));
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_batch_unauthorized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _minter) = setup_token(&env);

    let recipients = soroban_sdk::vec![&env, Address::generate(&env)];
    let amounts = soroban_sdk::vec![&env, 100i128];
    let stranger = Address::generate(&env);

    let result = client.try_mint_batch(&recipients, &amounts, &stranger);
    assert_eq!(result, Err(Ok(crate::errors::Error::Unauthorized)));
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_batch_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipients = soroban_sdk::vec![&env, Address::generate(&env)];
    let amounts = soroban_sdk::vec![&env, -1i128];

    let result = client.try_mint_batch(&recipients, &amounts, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_batch_paused_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    client.set_paused(&true);
    let recipients = soroban_sdk::vec![&env, Address::generate(&env)];
    let amounts = soroban_sdk::vec![&env, 100i128];

    let result = client.try_mint_batch(&recipients, &amounts, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::Paused)));
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_batch_length_mismatch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipients = soroban_sdk::vec![&env, Address::generate(&env), Address::generate(&env)];
    let amounts = soroban_sdk::vec![&env, 100i128];

    let result = client.try_mint_batch(&recipients, &amounts, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::BatchLengthMismatch)));
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_transfer_from_locked_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let sender = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.mint(&sender, &1000, &minter);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&sender, &spender, &500, &expiration);

    assert!(client.transfer_locked());
    let result = client.try_transfer_from(&spender, &sender, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
    assert_eq!(client.balance(&sender), 1000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_mint_overflow_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let max = i128::MAX;

    client.mint(&recipient, &max, &minter);
    assert_eq!(client.balance(&recipient), max);

    let result = client.try_mint(&recipient, &1, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::Overflow)));
    assert_eq!(client.balance(&recipient), max);
}

#[test]
fn test_mint_zero_amount_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_token(&env);
    let recipient = Address::generate(&env);

    let result = client.try_mint(&recipient, &0, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
}

#[test]
fn test_mint_negative_amount_rejected() {
    let env = Env::default();
    let (client, admin, minter) = setup_token(&env);
    let recipient = Address::generate(&env);

    let result = client.try_mint(&recipient, &(-100), &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::InvalidAmount)));
}

#[test]
fn test_mint_fractional_decimal_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Invoice");
    let symbol = SorobanString::from_str(&env, "INV");
    let invoice_id = Symbol::new(&env, "inv_001");

    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);

    let recipient = Address::generate(&env);
    client.mint(&recipient, &1, &minter);
    assert_eq!(client.balance(&recipient), 1);

    client.mint(&recipient, &9_999_999, &minter);
    assert_eq!(client.balance(&recipient), 10_000_000);
}

#[test]
fn test_mint_max_supply_overflow_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let recipient = Address::generate(&env);

    let max_i128 = i128::MAX;
    client.mint(&recipient, &max_i128, &minter);

    let result = client.try_mint(&recipient, &1, &minter);
    assert_eq!(result, Err(Ok(crate::errors::Error::Overflow)));
}

#[test]
fn test_persistent_storage_ttl_extension() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let recipient = Address::generate(&env);

    client.mint(&recipient, &1000, &minter);

    env.ledger().with_mut(|li| li.sequence_number += 100_000);

    let balance = client.balance(&recipient);
    assert_eq!(balance, 1000);
}

#[test]
fn test_storage_key_ttl_after_multiple_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    let recipient = Address::generate(&env);

    client.mint(&recipient, &1000, &minter);

    env.ledger().with_mut(|li| li.sequence_number += 50_000);

    let other = Address::generate(&env);
    client.approve(&recipient, &other, &500, &200_000);

    env.ledger().with_mut(|li| li.sequence_number += 50_000);

    let allowance = client.allowance(&recipient, &other);
    assert_eq!(allowance, 500);
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #457: property-based tests for fractional balance conservation.
// Invariant: for any sequence of splits/moves/burns, the sum of fractional
// balances plus burned supply equals minted supply at every step.
// ═══════════════════════════════════════════════════════════════════════════

/// Small deterministic LCG so failures are reproducible (no rand dep).
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state >> 33
}

fn lcg_amount(state: &mut u64, max: i128) -> i128 {
    1 + (lcg_next(state) as i128) % max
}

fn issue457_sum_balances(client: &InvoiceTokenClient, wallets: &soroban_sdk::Vec<Address>) -> i128 {
    let mut sum: i128 = 0;
    for w in wallets.iter() {
        sum += client.balance(&w);
    }
    sum
}

#[test]
fn issue457_mint_split_conserves_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Deterministic PRNG sweep: split 1_000_007 into 6 pseudo-random parts.
    let mut state: u64 = 0x4557;
    let mut wallets = soroban_sdk::vec![&env];
    for _ in 0..6 {
        wallets.push_back(Address::generate(&env));
    }
    let mut remaining: i128 = 1_000_007;
    let mut minted: i128 = 0;
    for i in 0..6 {
        let part = if i == 5 { remaining } else { lcg_amount(&mut state, remaining / 3).min(remaining - (5 - i as i128)) };
        client.mint(&wallets.get(i).unwrap(), &part, &minter);
        minted += part;
        remaining -= part;
    }
    assert_eq!(remaining, 0);
    assert_eq!(client.total_supply(), minted);
    assert_eq!(issue457_sum_balances(&client, &wallets), minted);
    assert_eq!(client.balance(&admin), 0);
}

#[test]
fn issue457_arbitrary_split_tree_conserves_balances() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Depth-3 binary split: each node forwards all it holds to two children
    // with pseudo-random fractional shares; the leaf sum must equal the root.
    let mut state: u64 = 0xBEEF;
    let root = Address::generate(&env);
    client.mint(&root, &9_999_983, &minter);
    client.set_transfer_locked(&_admin, &false);

    let mut frontier: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env, root.clone()];
    for _depth in 0..3 {
        let mut next: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env];
        for node in frontier.iter() {
            let bal = client.balance(&node);
            let left = Address::generate(&env);
            let right = Address::generate(&env);
            if bal >= 2 {
                let l_amt = 1 + (lcg_next(&mut state) as i128) % (bal - 1);
                let r_amt = bal - l_amt;
                if l_amt > 0 {
                    client.transfer(&node, &left, &l_amt);
                }
                if r_amt > 0 {
                    client.transfer(&node, &right, &r_amt);
                }
            }
            next.push_back(left);
            next.push_back(right);
        }
        frontier = next;
    }
    let mut leaf_sum: i128 = 0;
    for node in frontier.iter() {
        leaf_sum += client.balance(&node);
    }
    assert_eq!(leaf_sum, 9_999_983i128);
}

#[test]
fn issue457_transfers_with_fee_conserve_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    // A nonzero transfer fee moves value to the fee recipient; it must not
    // create or destroy supply. Sweep fee rates across the valid range.
    let mut state: u64 = 0xFEED;
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let supply: i128 = 50_000_009;
    client.mint(&alice, &supply, &minter);

    for fee_bps in [1i128, 13, 250, 1_000, 5_000, 9_999, 10_000] {
        client.set_fee_bps(&admin, &fee_bps);
        let amt = lcg_amount(&mut state, supply / 20);
        client.transfer(&alice, &bob, &amt);
        let total = client.balance(&alice)
            + client.balance(&bob)
            + client.balance(&admin);
        assert_eq!(total, supply, "conservation broke at fee_bps={}", fee_bps);
        assert_eq!(client.total_supply(), supply);
    }
}

#[test]
fn issue457_burn_reduces_supply_and_conserves_identity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let mut state: u64 = 0xACDC;
    let mut wallets = soroban_sdk::vec![&env];
    for _ in 0..4 {
        wallets.push_back(Address::generate(&env));
    }
    let mut minted: i128 = 0;
    for w in wallets.iter() {
        let amt = lcg_amount(&mut state, 100_000);
        client.mint(&w, &amt, &minter);
        minted += amt;
    }
    assert_eq!(client.total_supply(), minted);

    // Burn a pseudo-random slice from each wallet; supply must track exactly.
    let mut burned: i128 = 0;
    for w in wallets.iter() {
        let bal = client.balance(&w);
        let burn = 1 + (lcg_next(&mut state) as i128) % bal;
        client.burn(&w, &burn);
        burned += burn;
        assert_eq!(client.total_supply(), minted - burned);
    }
    assert_eq!(issue457_sum_balances(&client, &wallets), minted - burned);
}

#[test]
fn issue457_random_walk_mint_transfer_burn_conserves_every_step() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);
    client.set_transfer_locked(&admin, &false);

    // 60-step deterministic random walk over mint/transfer/burn; the
    // conservation identity must hold after every single step.
    let mut state: u64 = 0x5EED_0001;
    let mut wallets = soroban_sdk::vec![&env];
    for _ in 0..5 {
        wallets.push_back(Address::generate(&env));
    }
    let mut minted: i128 = 0;
    let mut burned: i128 = 0;

    for step in 0..60u64 {
        let wi = (lcg_next(&mut state) % 5) as usize;
        let w = &wallets.get(wi as u32).unwrap();
        let bal = client.balance(&w);
        match lcg_next(&mut state) % 3 {
            0 => {
                let amt = lcg_amount(&mut state, 50_000);
                client.mint(&w, &amt, &minter);
                minted += amt;
            }
            1 if bal >= 2 => {
                let mut ti = (lcg_next(&mut state) % 5) as usize;
                if ti == wi {
                    ti = (ti + 1) % 5;
                }
                let to = &wallets.get(ti as u32).unwrap();
                let amt = 1 + (lcg_next(&mut state) as i128) % (bal - 1);
                client.transfer(w, to, &amt);
            }
            _ if bal >= 1 => {
                let burn = 1 + (lcg_next(&mut state) as i128) % bal;
                client.burn(&w, &burn);
                burned += burn;
            }
            _ => {}
        }
        let held = issue457_sum_balances(&client, &wallets);
        assert_eq!(held, minted - burned, "step {}", step);
        assert_eq!(client.total_supply(), minted - burned, "step {}", step);
    }
}
