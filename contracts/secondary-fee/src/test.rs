#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

use crate::{Error, FeeReceipt, RegistryAssignment, Sector, SecondaryFee, SecondaryFeeClient};

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

#[allow(dead_code)]
struct Fixture {
    admin: Address,
    market: Address,
    treasury: Address,
    seller: Address,
    token: Address,
    token_admin: Address,
    contract: Address,
    client: SecondaryFeeClient<'static>,
}

fn setup<'a>(env: &Env, fee_bps: u32) -> Fixture {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SecondaryFee);
    let client = SecondaryFeeClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let market = Address::generate(env);
    let treasury = Address::generate(env);
    let seller = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token_id.address();

    client.initialize(&admin, &treasury, &fee_bps);
    client.set_secondary_market(&admin, &market);

    Fixture {
        admin,
        market,
        treasury,
        seller,
        token,
        token_admin,
        contract: contract_id,
        client: unsafe {
            core::mem::transmute::<_, SecondaryFeeClient<'static>>(client)
        },
    }
}

fn mint(env: &Env, fixture: &Fixture, to: &Address, amount: i128) {
    let asset = StellarAssetClient::new(env, &fixture.token);
    asset.mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

#[test]
fn test_collect_fee_from_market_balance_splits_correctly() {
    let env = Env::default();
    let f = setup(&env, 250); // 2.5%
    mint(&env, &f, &f.market, 1_000_000);

    let listing = Symbol::new(&env, "LST_001");
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &1_000_000,
        &f.seller,
        &f.token,
    );

    assert_eq!(
        receipt,
        FeeReceipt {
            sale_amount: 1_000_000,
            net_amount: 975_000,
            fee_amount: 25_000,
        }
    );
    assert_eq!(balance(&env, &f.token, &f.seller), 975_000);
    assert_eq!(balance(&env, &f.token, &f.treasury), 25_000);
    assert_eq!(balance(&env, &f.token, &f.market), 0);
    assert_eq!(balance(&env, &f.token, &f.contract), 0);
}

#[test]
fn test_collect_fee_from_predeposited_balance() {
    let env = Env::default();
    let f = setup(&env, 400); // 4%
    mint(&env, &f, &f.market, 500_000);

    // Market pre-deposits the sale amount into the fee contract.
    TokenClient::new(&env, &f.token).transfer(&f.market, &f.contract, &500_000);
    assert_eq!(balance(&env, &f.token, &f.contract), 500_000);

    let listing = Symbol::new(&env, "LST_002");
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &500_000,
        &f.seller,
        &f.token,
    );

    assert_eq!(receipt.net_amount, 480_000);
    assert_eq!(receipt.fee_amount, 20_000);
    assert_eq!(balance(&env, &f.token, &f.seller), 480_000);
    assert_eq!(balance(&env, &f.token, &f.treasury), 20_000);
    assert_eq!(balance(&env, &f.token, &f.contract), 0);
    assert_eq!(balance(&env, &f.token, &f.market), 0);
}

#[test]
fn test_fee_rounding_floors_and_conserves_total() {
    let env = Env::default();
    let f = setup(&env, 333); // 3.33%
    mint(&env, &f, &f.market, 1_000);

    let listing = Symbol::new(&env, "LST_RND");
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &1_000,
        &f.seller,
        &f.token,
    );

    // 1000 * 333 / 10000 = 33.3 -> floored to 33; seller absorbs the dust.
    assert_eq!(receipt.fee_amount, 33);
    assert_eq!(receipt.net_amount, 967);
    assert_eq!(receipt.fee_amount + receipt.net_amount, receipt.sale_amount);
    assert_eq!(balance(&env, &f.token, &f.seller), 967);
    assert_eq!(balance(&env, &f.token, &f.treasury), 33);
}

#[test]
fn test_zero_fee_sends_full_amount_to_seller() {
    let env = Env::default();
    let f = setup(&env, 0);
    mint(&env, &f, &f.market, 75_000);

    let listing = Symbol::new(&env, "LST_FREE");
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &75_000,
        &f.seller,
        &f.token,
    );

    assert_eq!(receipt.fee_amount, 0);
    assert_eq!(receipt.net_amount, 75_000);
    assert_eq!(balance(&env, &f.token, &f.seller), 75_000);
    assert_eq!(balance(&env, &f.token, &f.treasury), 0);
}

#[test]
fn test_full_fee_sends_everything_to_treasury() {
    let env = Env::default();
    let f = setup(&env, 10_000);
    mint(&env, &f, &f.market, 60_000);

    let listing = Symbol::new(&env, "LST_ALL");
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &60_000,
        &f.seller,
        &f.token,
    );

    assert_eq!(receipt.net_amount, 0);
    assert_eq!(receipt.fee_amount, 60_000);
    assert_eq!(balance(&env, &f.token, &f.seller), 0);
    assert_eq!(balance(&env, &f.token, &f.treasury), 60_000);
}

#[test]
fn test_fee_rate_update_and_treasury_rerouting() {
    let env = Env::default();
    let f = setup(&env, 100);
    mint(&env, &f, &f.market, 100_000);
    let listing = Symbol::new(&env, "LST_UPD");

    let first = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &100_000,
        &f.seller,
        &f.token,
    );
    assert_eq!(first.fee_amount, 1_000);

    // Admin updates fee rate and treasury.
    f.client.set_fee_bps(&f.admin, &500);
    assert_eq!(f.client.get_fee_bps(), 500);
    let new_treasury = Address::generate(&env);
    f.client.set_treasury(&f.admin, &new_treasury);
    assert_eq!(f.client.get_treasury(), new_treasury);

    // Mint more tokens to market for the second sale.
    mint(&env, &f, &f.market, 100_000);

    let second = f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &100_000,
        &f.seller,
        &f.token,
    );
    assert_eq!(second.fee_amount, 5_000);
    assert_eq!(balance(&env, &f.token, &new_treasury), 5_000);
    // Old treasury only received the first sale's fee.
    assert_eq!(balance(&env, &f.token, &f.treasury), 1_000);
}

#[test]
fn test_collect_requires_bound_market_and_correct_caller() {
    let env = Env::default();
    let f = setup(&env, 250);
    mint(&env, &f, &f.market, 100_000);

    // Wrong caller, even with the amount available.
    let impostor = Address::generate(&env);
    let listing = Symbol::new(&env, "LST_AUTH");
    assert_eq!(
        f.client.try_collect_secondary_fee(
            &impostor,
            &listing,
            &100_000,
            &f.seller,
            &f.token,
        ),
        Err(Ok(Error::Unauthorized))
    );

    // Contract with no market bound yet refuses collection.
    let fresh_id = env.register_contract(None, SecondaryFee);
    let fresh = SecondaryFeeClient::new(&env, &fresh_id);
    fresh.initialize(&f.admin, &f.treasury, &250);
    assert_eq!(
        fresh.try_collect_secondary_fee(
            &f.market,
            &listing,
            &100_000,
            &f.seller,
            &f.token,
        ),
        Err(Ok(Error::SecondaryMarketNotSet))
    );
}

#[test]
fn test_admin_updates_are_restricted_and_validated() {
    let env = Env::default();
    let f = setup(&env, 250);

    let impostor = Address::generate(&env);
    assert_eq!(
        f.client.try_set_fee_bps(&impostor, &300),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_treasury(&impostor, &f.seller),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_secondary_market(&impostor, &f.seller),
        Err(Ok(Error::Unauthorized))
    );

    // Fee rate must stay within [0, 10000].
    assert_eq!(
        f.client.try_set_fee_bps(&f.admin, &10_001),
        Err(Ok(Error::InvalidFeeBps))
    );

    // Missing auth is rejected.
    env.set_auths(&[]);
    assert!(f.client.try_set_fee_bps(&f.admin, &300).is_err());
}

#[test]
fn test_invalid_sale_amounts_and_insufficient_balance_rejected() {
    let env = Env::default();
    let f = setup(&env, 250);
    mint(&env, &f, &f.market, 100);

    let listing = Symbol::new(&env, "LST_BAD");
    assert_eq!(
        f.client.try_collect_secondary_fee(
            &f.market,
            &listing,
            &0,
            &f.seller,
            &f.token,
        ),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        f.client.try_collect_secondary_fee(
            &f.market,
            &listing,
            &-5,
            &f.seller,
            &f.token,
        ),
        Err(Ok(Error::InvalidAmount))
    );
    // Sale larger than both the contract and market balances.
    assert_eq!(
        f.client.try_collect_secondary_fee(
            &f.market,
            &listing,
            &1_000,
            &f.seller,
            &f.token,
        ),
        Err(Ok(Error::InsufficientBalance))
    );
}

#[test]
fn test_secondary_fee_collected_event_payload() {
    let env = Env::default();
    let f = setup(&env, 200);
    mint(&env, &f, &f.market, 10_000);
    env.ledger().set_timestamp(777_000);

    let listing = Symbol::new(&env, "LST_EVT");
    f.client.collect_secondary_fee(
        &f.market,
        &listing,
        &10_000,
        &f.seller,
        &f.token,
    );

    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (contract_addr, topics, data) = parse_event(&env, event);
    assert_eq!(contract_addr, f.contract);

    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        Symbol::new(&env, "SecondaryFeeCollected")
    );
    let event_listing: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_listing, listing);
    let event_seller: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_seller, f.seller);

    let (event_token, sale, fee, net, timestamp): (Address, i128, i128, i128, u64) =
        data.try_into_val(&env).unwrap();
    assert_eq!(event_token, f.token);
    assert_eq!(sale, 10_000);
    assert_eq!(fee, 200);
    assert_eq!(net, 9_800);
    assert_eq!(timestamp, 777_000);
}

#[test]
fn test_preview_matches_collection() {
    let env = Env::default();
    let f = setup(&env, 150);
    mint(&env, &f, &f.market, 200_000);

    let preview = f.client.preview_fee(&200_000);
    let receipt = f.client.collect_secondary_fee(
        &f.market,
        &Symbol::new(&env, "LST_PRE"),
        &200_000,
        &f.seller,
        &f.token,
    );
    assert_eq!(preview, receipt);
    assert_eq!(preview.net_amount, 197_000);
    assert_eq!(preview.fee_amount, 3_000);
}

#[test]
fn test_initialize_validation_and_double_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SecondaryFee);
    let client = SecondaryFeeClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    assert_eq!(
        client.try_initialize(&admin, &treasury, &10_001),
        Err(Ok(Error::InvalidFeeBps))
    );

    client.initialize(&admin, &treasury, &250);
    assert_eq!(
        client.try_initialize(&admin, &treasury, &250),
        Err(Ok(Error::AlreadyInit))
    );

    // Zero-address treasury is rejected.
    let zero_str = String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    let fresh_id = env.register_contract(None, SecondaryFee);
    let fresh = SecondaryFeeClient::new(&env, &fresh_id);
    assert_eq!(
        fresh.try_initialize(&admin, &zero, &250),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_uninitialized_calls_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SecondaryFee);
    let client = SecondaryFeeClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let market = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);

    assert_eq!(client.try_get_fee_bps(), Err(Ok(Error::NotInit)));
    assert_eq!(
        client.try_collect_secondary_fee(
            &market,
            &Symbol::new(&env, "LST_NI"),
            &100,
            &seller,
            &token,
        ),
        Err(Ok(Error::NotInit))
    );
    assert_eq!(
        client.try_set_fee_bps(&admin, &100),
        Err(Ok(Error::NotInit))
    );
}

// ── Listing fee ───────────────────────────────────────────────────────────────

#[contract]
struct MockInvoiceRegistry;

#[contractimpl]
impl MockInvoiceRegistry {
    pub fn set_sector(env: Env, invoice_id: BytesN<32>, sector: Sector) {
        env.storage().instance().set(
            &invoice_id,
            &RegistryAssignment {
                invoice_id: invoice_id.clone(),
                category: sector,
                risk_tier: 0,
            },
        );
    }

    pub fn get_category(env: Env, invoice_id: BytesN<32>) -> RegistryAssignment {
        env.storage().instance().get(&invoice_id).unwrap()
    }
}

/// Fixture with a registry bound and a per-fraction fee of 10 for SMEs.
fn setup_listing(env: &Env) -> (Fixture, BytesN<32>) {
    let f = setup(env, 250);
    let registry_id = env.register_contract(None, MockInvoiceRegistry);
    let invoice = BytesN::from_array(env, &[7u8; 32]);
    MockInvoiceRegistryClient::new(env, &registry_id).set_sector(&invoice, &Sector::Sme);
    f.client.set_invoice_registry(&f.admin, &registry_id);
    f.client.set_listing_fee(&f.admin, &Sector::Sme, &10);
    (f, invoice)
}

#[test]
fn test_get_listing_fee_uses_sector_rate() {
    let env = Env::default();
    let (f, invoice) = setup_listing(&env);

    assert_eq!(f.client.get_listing_fee(&invoice, &1), 10);
    assert_eq!(f.client.get_listing_fee(&invoice, &250), 2_500);

    // A sector without a configured rate carries no listing fee.
    f.client.set_listing_fee(&f.admin, &Sector::Sme, &0);
    assert_eq!(f.client.get_listing_fee(&invoice, &250), 0);

    assert_eq!(
        f.client.try_get_listing_fee(&invoice, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        f.client
            .try_get_listing_fee(&BytesN::from_array(&env, &[8u8; 32]), &1),
        Err(Ok(Error::InvoiceNotRegistered))
    );
}

#[test]
fn test_collect_listing_fee_routes_fee_to_treasury_and_emits_event() {
    let env = Env::default();
    let (f, invoice) = setup_listing(&env);
    mint(&env, &f, &f.seller, 10_000);

    let fee = f
        .client
        .collect_listing_fee(&f.market, &f.seller, &invoice, &250, &f.token);
    assert_eq!(fee, 2_500);
    assert_eq!(balance(&env, &f.token, &f.seller), 7_500);
    assert_eq!(balance(&env, &f.token, &f.treasury), 2_500);

    let mut found = false;
    for e in env.events().all().events().iter() {
        let (_, topics, data) = parse_event(&env, e);
        let name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
        if name == Symbol::new(&env, "ListingFeeCollected") {
            found = true;
            let inv: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
            let seller: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
            let (token, amount, fee_amount, _ts): (Address, i128, i128, u64) =
                data.try_into_val(&env).unwrap();
            assert_eq!(inv, invoice);
            assert_eq!(seller, f.seller);
            assert_eq!(token, f.token);
            assert_eq!(amount, 250);
            assert_eq!(fee_amount, 2_500);
        }
    }
    assert!(found, "expected ListingFeeCollected event");
}

#[test]
fn test_collect_listing_fee_rejected_when_seller_cannot_cover_fee() {
    let env = Env::default();
    let (f, invoice) = setup_listing(&env);
    mint(&env, &f, &f.seller, 2_499);

    let res = f
        .client
        .try_collect_listing_fee(&f.market, &f.seller, &invoice, &250, &f.token);
    assert_eq!(res, Err(Ok(Error::InsufficientBalance)));
    assert_eq!(balance(&env, &f.token, &f.seller), 2_499);
    assert_eq!(balance(&env, &f.token, &f.treasury), 0);
}

#[test]
fn test_collect_listing_fee_only_callable_by_market() {
    let env = Env::default();
    let (f, invoice) = setup_listing(&env);
    mint(&env, &f, &f.seller, 10_000);

    let impostor = Address::generate(&env);
    let res = f
        .client
        .try_collect_listing_fee(&impostor, &f.seller, &invoice, &250, &f.token);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_listing_fee_setters_are_admin_only_and_reject_negative() {
    let env = Env::default();
    let (f, _invoice) = setup_listing(&env);
    let impostor = Address::generate(&env);

    assert_eq!(
        f.client.try_set_listing_fee(&impostor, &Sector::Sme, &5),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_invoice_registry(&impostor, &impostor),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_listing_fee(&f.admin, &Sector::Sme, &-1),
        Err(Ok(Error::InvalidAmount))
    );
}
