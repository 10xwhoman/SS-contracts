//! Tests for per-invoice minimum/maximum funding thresholds:
//! `create_invoice` thresholds, `invest` capacity cap, `release_to_issuer`
//! gating, `get_funding_status`, and the one-shot `ThresholdReached` event.
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use invoice_escrow::{Error, EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};

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

struct Fixture<'a> {
    admin: Address,
    seller: Address,
    token: Address,
    contract: Address,
    client: InvoiceEscrowClient<'a>,
}

fn setup(env: &Env) -> Fixture<'_> {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin, &300);

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    Fixture {
        admin,
        seller: Address::generate(env),
        token,
        contract: contract_id,
        client,
    }
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[allow(clippy::too_many_arguments)]
fn create_threshold_invoice(
    env: &Env,
    f: &Fixture,
    id: &BytesN<32>,
    target: i128,
    min: i128,
    max: i128,
) {
    f.client
        .create_invoice(id, &f.seller, &target, &min, &max, &1000u32, &0i128, &None, &f.token);
    // Sanity: thresholds are stored on the invoice.
    let status = f.client.get_funding_status(id);
    assert_eq!(status.min_threshold, min);
    assert_eq!(status.max_threshold, max);
}

#[test]
fn test_create_invoice_stores_thresholds() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 1);

    f.client.create_invoice(
        &id,
        &f.seller,
        &80_000i128,
        &40_000i128,
        &100_000i128,
        &1000u32,
        &0i128,
        &None,
        &f.token,
    );

    let status = f.client.get_funding_status(&id);
    assert_eq!(status.raised, 0);
    assert_eq!(status.min_threshold, 40_000);
    assert_eq!(status.max_threshold, 100_000);
    assert_eq!(status.percent_filled_bps, 0);
}

#[test]
fn test_invest_rejects_above_max_and_accepts_partial_up_to_capacity() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 2);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let third = Address::generate(&env);
    mint(&env, &f.token, &first, 500_000);
    mint(&env, &f.token, &second, 500_000);
    mint(&env, &f.token, &third, 500_000);

    // target 80k, min 40k, max 100k.
    create_threshold_invoice(&env, &f, &id, 80_000, 40_000, 100_000);

    // Partial investment below the target is accepted.
    f.client.invest(&id, &first, &60_000i128);
    assert_eq!(f.client.get_funding_status(&id).raised, 60_000);

    // 60k + 41k = 101k > max: rejected.
    assert_eq!(
        f.client.try_invest(&id, &second, &41_000i128),
        Err(Ok(Error::MaxFundingThresholdExceeded))
    );

    // The exact remaining capacity (40k) is accepted.
    f.client.invest(&id, &second, &40_000i128);
    assert_eq!(f.client.get_funding_status(&id).raised, 100_000);
    assert_eq!(balance(&env, &f.token, &f.contract), 100_000);

    // Capacity exhausted: any further investment is rejected.
    assert_eq!(
        f.client.try_invest(&id, &third, &1i128),
        Err(Ok(Error::MaxFundingThresholdExceeded))
    );

    // Above-cap attempts never move funds.
    assert_eq!(balance(&env, &f.token, &third), 500_000);
}

#[test]
fn test_threshold_reached_event_emitted_exactly_once() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 3);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    mint(&env, &f.token, &first, 500_000);
    mint(&env, &f.token, &second, 500_000);

    create_threshold_invoice(&env, &f, &id, 80_000, 50_000, 100_000);

    // Helper to collect ThresholdReached events from the last invocation.
    let mut collect_threshold_events = |env: &Env| -> Vec<(BytesN<32>, i128, i128)> {
        let mut found = Vec::new(env);
        for event in env.events().all().events() {
            let (_, topics, data) = parse_event(env, event);
            let first_topic: Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
            if first_topic == Symbol::new(env, "ThresholdReached") {
                let event_invoice: BytesN<32> = topics.get(1).unwrap().try_into_val(env).unwrap();
                let (min_threshold, total_raised): (i128, i128) =
                    data.try_into_val(env).unwrap();
                found.push_back((event_invoice, min_threshold, total_raised));
            }
        }
        found
    };

    // Below the minimum: no event.
    f.client.invest(&id, &first, &20_000i128);
    assert_eq!(collect_threshold_events(&env).len(), 0);

    // Crossing exactly at 50k emits the event once.
    f.client.invest(&id, &first, &30_000i128);
    let events = collect_threshold_events(&env);
    assert_eq!(events.len(), 1);
    let (event_invoice, min_threshold, total_raised) = events.get(0).unwrap();
    assert_eq!(event_invoice, id);
    assert_eq!(min_threshold, 50_000);
    assert_eq!(total_raised, 50_000);

    // Further invest and top-up beyond the minimum emit nothing more.
    f.client.invest(&id, &second, &20_000i128);
    assert_eq!(collect_threshold_events(&env).len(), 0);
    f.client.top_up(&first, &id, &10_000i128);
    assert_eq!(collect_threshold_events(&env).len(), 0);
}

#[test]
fn test_release_blocked_below_min_then_allowed_at_min() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 4);
    let investor = Address::generate(&env);
    mint(&env, &f.token, &investor, 500_000);

    create_threshold_invoice(&env, &f, &id, 80_000, 50_000, 100_000);

    f.client.invest(&id, &investor, &40_000i128);
    assert_eq!(
        f.client.try_release_to_issuer(&id),
        Err(Ok(Error::FundingThresholdNotReached))
    );
    assert_eq!(balance(&env, &f.token, &f.seller), 0);
    assert_eq!(balance(&env, &f.token, &f.contract), 40_000);

    // Reaching the minimum unblocks the release.
    f.client.invest(&id, &investor, &10_000i128);
    f.client.release_to_issuer(&id);
    assert_eq!(balance(&env, &f.token, &f.seller), 50_000);
    assert_eq!(balance(&env, &f.token, &f.contract), 0);

    // Release is one-shot: the invoice is Funded now.
    assert_eq!(
        f.client.try_release_to_issuer(&id),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
    assert_eq!(
        f.client.try_invest(&id, &investor, &1i128),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
}

#[test]
fn test_release_to_issuer_requires_admin_auth() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 5);
    let investor = Address::generate(&env);
    mint(&env, &f.token, &investor, 500_000);
    create_threshold_invoice(&env, &f, &id, 80_000, 50_000, 100_000);
    f.client.invest(&id, &investor, &50_000i128);

    env.set_auths(&[]);
    assert!(f.client.try_release_to_issuer(&id).is_err());
}

#[test]
fn test_get_funding_status_percent_filled_at_all_levels() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 6);
    let investor = Address::generate(&env);
    mint(&env, &f.token, &investor, 500_000);

    // target 80k, min 40k, max 120k.
    create_threshold_invoice(&env, &f, &id, 80_000, 40_000, 120_000);

    f.client.invest(&id, &investor, &20_000i128);
    let s = f.client.get_funding_status(&id);
    assert_eq!((s.raised, s.min_threshold, s.max_threshold, s.percent_filled_bps), (20_000, 40_000, 120_000, 2_500));

    f.client.invest(&id, &investor, &40_000i128);
    let s = f.client.get_funding_status(&id);
    assert_eq!((s.raised, s.percent_filled_bps), (60_000, 7_500));

    f.client.invest(&id, &investor, &20_000i128);
    let s = f.client.get_funding_status(&id);
    assert_eq!((s.raised, s.percent_filled_bps), (80_000, 10_000));

    // Overfunding above the target is capped at 100% in the view.
    f.client.invest(&id, &investor, &40_000i128);
    let s = f.client.get_funding_status(&id);
    assert_eq!((s.raised, s.percent_filled_bps), (120_000, 10_000));
}

#[test]
fn test_legacy_create_invoice_defaults_min_and_max_to_target() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 7);
    let investor = Address::generate(&env);
    mint(&env, &f.token, &investor, 500_000);

    f.client
        .create_invoice_legacy(&id, &f.seller, &80_000i128, &1000u32, &0i128, &None, &f.token);

    let status = f.client.get_funding_status(&id);
    assert_eq!(status.min_threshold, 80_000);
    assert_eq!(status.max_threshold, 80_000);

    // Legacy invoices cannot exceed the target anymore.
    f.client.invest(&id, &investor, &79_999i128);
    assert_eq!(
        f.client.try_invest(&id, &investor, &2i128),
        Err(Ok(Error::MaxFundingThresholdExceeded))
    );
    f.client.invest(&id, &investor, &1i128);

    // Release requires the full target for legacy invoices.
    f.client.release_to_issuer(&id);
    assert_eq!(balance(&env, &f.token, &f.seller), 80_000);
}

#[test]
fn test_invalid_threshold_configuration_rejected() {
    let env = Env::default();
    let f = setup(&env);

    // min == 0.
    assert_eq!(
        f.client.try_create_invoice(
            &invoice_id(&env, 8),
            &f.seller,
            &80_000i128,
            &0i128,
            &100_000i128,
            &1000u32,
            &0i128,
            &None,
            &f.token,
        ),
        Err(Ok(Error::InvalidFundingThreshold))
    );

    // min > target.
    assert_eq!(
        f.client.try_create_invoice(
            &invoice_id(&env, 9),
            &f.seller,
            &50_000i128,
            &60_000i128,
            &70_000i128,
            &1000u32,
            &0i128,
            &None,
            &f.token,
        ),
        Err(Ok(Error::InvalidFundingThreshold))
    );

    // target > max.
    assert_eq!(
        f.client.try_create_invoice(
            &invoice_id(&env, 10),
            &f.seller,
            &80_000i128,
            &40_000i128,
            &70_000i128,
            &1000u32,
            &0i128,
            &None,
            &f.token,
        ),
        Err(Ok(Error::InvalidFundingThreshold))
    );

    // min == target == max is valid.
    f.client.create_invoice(
        &invoice_id(&env, 11),
        &f.seller,
        &80_000i128,
        &80_000i128,
        &80_000i128,
        &1000u32,
        &0i128,
        &None,
        &f.token,
    );
}

#[test]
fn test_release_to_issuer_on_registered_invoice_requires_target() {
    let env = Env::default();
    let f = setup(&env);
    let id = invoice_id(&env, 12);
    let investor = Address::generate(&env);
    mint(&env, &f.token, &investor, 500_000);

    f.client.register_invoice(
        &id,
        &100_000i128,
        &80_000i128,
        &500u32,
        &1000u32,
        &BytesN::from_array(&env, &[9u8; 32]),
    );

    f.client.invest(&id, &investor, &20_000i128);
    assert_eq!(
        f.client.try_release_to_issuer(&id),
        Err(Ok(Error::FundingThresholdNotReached))
    );

    f.client.invest(&id, &investor, &60_000i128);
    f.client.release_to_issuer(&id);
    assert_eq!(
        f.client.get_invoice_record(&id).status,
        EscrowStatus::Funded
    );

    // Registered invoices report the target as both thresholds.
    let status = f.client.get_funding_status(&id);
    assert_eq!(status.raised, 80_000);
    assert_eq!(status.min_threshold, 80_000);
    assert_eq!(status.max_threshold, 80_000);
    assert_eq!(status.percent_filled_bps, 10_000);
}

#[test]
fn test_get_funding_status_unknown_invoice_rejected() {
    let env = Env::default();
    let f = setup(&env);
    assert_eq!(
        f.client.try_get_funding_status(&invoice_id(&env, 13)),
        Err(Ok(Error::EscrowNotFound))
    );
}
