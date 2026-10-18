#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{
    Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

use crate::types::{InvoiceCategory, RiskTier};
use crate::{Error, InvoiceRegistry, InvoiceRegistryClient};

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

fn setup<'a>(env: &Env) -> (Address, InvoiceRegistryClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceRegistry);
    let client = InvoiceRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn test_set_and_get_category_all_combinations() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let categories = [
        InvoiceCategory::TradeFinance,
        InvoiceCategory::RealEstate,
        InvoiceCategory::Sme,
        InvoiceCategory::Infrastructure,
    ];
    let tiers = [RiskTier::Low, RiskTier::Medium, RiskTier::High];

    let mut seed = 1u8;
    for category in categories.iter() {
        for tier in tiers.iter() {
            let id = invoice_id(&env, seed);
            client.set_category(&admin, &id, category, tier);
            seed += 1;
        }
    }

    seed = 1u8;
    for category in categories.iter() {
        for tier in tiers.iter() {
            let id = invoice_id(&env, seed);
            let stored = client.get_category(&id);
            assert_eq!(stored.invoice_id, id);
            assert_eq!(stored.category, *category);
            assert_eq!(stored.risk_tier, *tier);
            assert_eq!(client.get_risk_tier(&id), *tier);
            seed += 1;
        }
    }
}

#[test]
fn test_get_invoices_by_category_returns_exact_ids() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let trade_a = invoice_id(&env, 10);
    let trade_b = invoice_id(&env, 11);
    let real_estate = invoice_id(&env, 12);
    let sme = invoice_id(&env, 13);

    client.set_category(&admin, &trade_a, &InvoiceCategory::TradeFinance, &RiskTier::Low);
    client.set_category(&admin, &trade_b, &InvoiceCategory::TradeFinance, &RiskTier::High);
    client.set_category(&admin, &real_estate, &InvoiceCategory::RealEstate, &RiskTier::Medium);
    client.set_category(&admin, &sme, &InvoiceCategory::Sme, &RiskTier::Low);

    let trade = client.get_invoices_by_category(&InvoiceCategory::TradeFinance);
    assert_eq!(trade.len(), 2);
    assert_eq!(trade.get(0).unwrap(), trade_a);
    assert_eq!(trade.get(1).unwrap(), trade_b);

    let re = client.get_invoices_by_category(&InvoiceCategory::RealEstate);
    assert_eq!(re.len(), 1);
    assert_eq!(re.get(0).unwrap(), real_estate);

    let sme_list = client.get_invoices_by_category(&InvoiceCategory::Sme);
    assert_eq!(sme_list.len(), 1);
    assert_eq!(sme_list.get(0).unwrap(), sme);

    // Categories with no invoices return an empty vec.
    let infra = client.get_invoices_by_category(&InvoiceCategory::Infrastructure);
    assert_eq!(infra.len(), 0);
}

#[test]
fn test_reclassification_moves_invoice_between_indexes() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let id = invoice_id(&env, 20);
    client.set_category(&admin, &id, &InvoiceCategory::Sme, &RiskTier::Medium);
    assert_eq!(
        client
            .get_invoices_by_category(&InvoiceCategory::Sme)
            .len(),
        1
    );

    // Re-classifying moves it out of `Sme` and into `TradeFinance`.
    client.set_category(&admin, &id, &InvoiceCategory::TradeFinance, &RiskTier::Low);

    assert_eq!(
        client
            .get_invoices_by_category(&InvoiceCategory::Sme)
            .len(),
        0
    );
    let trade = client.get_invoices_by_category(&InvoiceCategory::TradeFinance);
    assert_eq!(trade.len(), 1);
    assert_eq!(trade.get(0).unwrap(), id);

    let stored = client.get_category(&id);
    assert_eq!(stored.category, InvoiceCategory::TradeFinance);
    assert_eq!(stored.risk_tier, RiskTier::Low);
}

#[test]
fn test_same_category_update_changes_tier_without_duplicating() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let id = invoice_id(&env, 30);
    client.set_category(&admin, &id, &InvoiceCategory::RealEstate, &RiskTier::Low);
    client.set_category(&admin, &id, &InvoiceCategory::RealEstate, &RiskTier::High);

    let list = client.get_invoices_by_category(&InvoiceCategory::RealEstate);
    assert_eq!(list.len(), 1);
    assert_eq!(client.get_risk_tier(&id), RiskTier::High);
}

#[test]
fn test_category_assigned_event_emitted_with_invoice_and_category() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let id = invoice_id(&env, 40);
    client.set_category(&admin, &id, &InvoiceCategory::Infrastructure, &RiskTier::High);

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(contract_addr, client.address);

    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        Symbol::new(&env, "CategoryAssigned")
    );
    let event_invoice: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_invoice, id);

    let (category, risk_tier): (u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(category, InvoiceCategory::Infrastructure as u32);
    assert_eq!(risk_tier, RiskTier::High as u32);
}

#[test]
fn test_set_category_wrong_admin_rejected() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let impostor = Address::generate(&env);
    let id = invoice_id(&env, 50);
    assert_eq!(
        client.try_set_category(
            &impostor,
            &id,
            &InvoiceCategory::Sme,
            &RiskTier::Low
        ),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_set_category_requires_admin_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let id = invoice_id(&env, 51);
    assert!(client
        .try_set_category(&admin, &id, &InvoiceCategory::Sme, &RiskTier::Low)
        .is_err());
}

#[test]
fn test_get_category_unknown_invoice_rejected() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let id = invoice_id(&env, 60);
    assert_eq!(client.try_get_category(&id), Err(Ok(Error::NotRegistered)));
    assert_eq!(client.try_get_risk_tier(&id), Err(Ok(Error::NotRegistered)));
}

#[test]
fn test_remove_category_clears_assignment_and_index() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let id = invoice_id(&env, 70);
    client.set_category(&admin, &id, &InvoiceCategory::TradeFinance, &RiskTier::Low);
    client.remove_category(&admin, &id);

    assert_eq!(client.try_get_category(&id), Err(Ok(Error::NotRegistered)));
    assert_eq!(
        client
            .get_invoices_by_category(&InvoiceCategory::TradeFinance)
            .len(),
        0
    );

    // Removing twice fails with NotRegistered.
    assert_eq!(
        client.try_remove_category(&admin, &id),
        Err(Ok(Error::NotRegistered))
    );
}

#[test]
fn test_initialize_twice_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceRegistry);
    let client = InvoiceRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(
        client.try_initialize(&admin),
        Err(Ok(Error::AlreadyInit))
    );
}

#[test]
fn test_uninitialized_calls_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceRegistry);
    let client = InvoiceRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let id = invoice_id(&env, 80);
    assert_eq!(
        client.try_set_category(&admin, &id, &InvoiceCategory::Sme, &RiskTier::Low),
        Err(Ok(Error::NotInit))
    );
    assert_eq!(client.try_get_admin(), Err(Ok(Error::NotInit)));
}

#[test]
fn test_mark_under_review_flags_invoice_and_admin_clears() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let authority = Address::generate(&env);
    let id = invoice_id(&env, 90);

    assert!(!client.is_under_review(&id));
    // No authority bound yet.
    assert_eq!(
        client.try_mark_under_review(&authority, &id),
        Err(Ok(Error::Unauthorized))
    );

    client.set_review_authority(&admin, &authority);
    client.mark_under_review(&authority, &id);
    assert!(client.is_under_review(&id));

    let impostor = Address::generate(&env);
    assert_eq!(
        client.try_mark_under_review(&impostor, &invoice_id(&env, 91)),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_clear_under_review(&impostor, &id),
        Err(Ok(Error::Unauthorized))
    );

    client.clear_under_review(&admin, &id);
    assert!(!client.is_under_review(&id));
}
