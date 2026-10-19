//! Multi-currency investment tests: FX oracle normalisation of `invest_with_currency`.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};

const NOW: u64 = 1_000_000;

#[contract]
struct MockFxOracle;

#[contractimpl]
impl MockFxOracle {
    pub fn set_price(env: Env, currency: Symbol, price: i128, decimals: u32, timestamp: u64) {
        env.storage().instance().set(
            &currency,
            &PriceData {
                price,
                decimals,
                timestamp,
            },
        );
    }

    pub fn get_price(env: Env, currency: Symbol) -> PriceData {
        env.storage().instance().get(&currency).unwrap()
    }
}

struct Fx<'a> {
    env: Env,
    client: InvoiceEscrowClient<'a>,
    oracle: MockFxOracleClient<'a>,
    admin: Address,
    invoice_id: BytesN<32>,
    eurc: Address,
    ngnc: Address,
}

fn setup<'a>() -> Fx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(NOW);

    let contract_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &300);

    let oracle_id = env.register(MockFxOracle, ());
    let oracle = MockFxOracleClient::new(&env, &oracle_id);
    client.set_fx_oracle(&admin, &oracle_id);

    let base = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let eurc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let ngnc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    client.set_supported_currency(&admin, &Symbol::new(&env, "EURC"), &eurc);
    client.set_supported_currency(&admin, &Symbol::new(&env, "NGNC"), &ngnc);

    let invoice_id = BytesN::from_array(&env, &[93u8; 32]);
    client.create_invoice(
        &invoice_id,
        &Address::generate(&env),
        &1_000_000,
        &100_000,
        &1_000_000,
        &1_000,
        &0,
        &None,
        &base,
    );

    // 1 EURC = 1.08 base, 1 NGNC = 0.001 base (8 decimals).
    oracle.set_price(&Symbol::new(&env, "EURC"), &108_000_000, &8, &NOW);
    oracle.set_price(&Symbol::new(&env, "NGNC"), &100_000, &8, &NOW);

    Fx {
        env,
        client,
        oracle,
        admin,
        invoice_id,
        eurc,
        ngnc,
    }
}

fn fund(f: &Fx, token: &Address, to: &Address, amount: i128) {
    AssetClient::new(&f.env, token).mint(to, &amount);
}

#[test]
fn invest_normalises_amount_with_oracle_rate_and_stores_record() {
    let f = setup();
    let investor = Address::generate(&f.env);
    let eurc = Symbol::new(&f.env, "EURC");
    fund(&f, &f.eurc, &investor, 100_000);

    let base = f
        .client
        .invest_with_currency(&f.invoice_id, &investor, &100_000, &eurc);
    assert_eq!(base, 108_000);
    assert_eq!(f.client.get_investor_position(&f.invoice_id, &investor), 108_000);
    assert_eq!(f.client.get_invoice(&f.invoice_id).total_raised, 108_000);

    // The investor paid in EURC, which the escrow now holds.
    assert_eq!(TokenClient::new(&f.env, &f.eurc).balance(&investor), 0);

    let records = f.client.get_investment_records(&f.invoice_id, &investor);
    assert_eq!(records.len(), 1);
    let record = records.get(0).unwrap();
    assert_eq!(record.currency, eurc);
    assert_eq!(record.amount, 100_000);
    assert_eq!(record.rate, 108_000_000);
    assert_eq!(record.decimals, 8);
    assert_eq!(record.base_amount, 108_000);
    assert_eq!(record.timestamp, NOW);
}

#[test]
fn stale_oracle_rate_is_rejected() {
    let f = setup();
    let investor = Address::generate(&f.env);
    fund(&f, &f.eurc, &investor, 100_000);

    f.env
        .ledger()
        .set_timestamp(NOW + OraclePriceAdapter::DEFAULT_MAX_STALENESS_SECS + 1);
    let res = f.client.try_invest_with_currency(
        &f.invoice_id,
        &investor,
        &100_000,
        &Symbol::new(&f.env, "EURC"),
    );
    assert_eq!(res, Err(Ok(Error::StalePrice)));

    // A refreshed rate is accepted again.
    f.oracle.set_price(
        &Symbol::new(&f.env, "EURC"),
        &108_000_000,
        &8,
        &(NOW + OraclePriceAdapter::DEFAULT_MAX_STALENESS_SECS + 1),
    );
    assert!(f
        .client
        .try_invest_with_currency(
            &f.invoice_id,
            &investor,
            &100_000,
            &Symbol::new(&f.env, "EURC"),
        )
        .is_ok());
}

#[test]
fn unlisted_currency_is_rejected() {
    let f = setup();
    let investor = Address::generate(&f.env);

    let res = f.client.try_invest_with_currency(
        &f.invoice_id,
        &investor,
        &100_000,
        &Symbol::new(&f.env, "GBPC"),
    );
    assert_eq!(res, Err(Ok(Error::UnsupportedCurrency)));

    // Removing a listed currency makes it unsupported too.
    let eurc = Symbol::new(&f.env, "EURC");
    f.client.remove_supported_currency(&f.admin, &eurc);
    assert_eq!(f.client.get_supported_currency(&eurc), None);
    let res = f
        .client
        .try_invest_with_currency(&f.invoice_id, &investor, &100_000, &eurc);
    assert_eq!(res, Err(Ok(Error::UnsupportedCurrency)));
}

#[test]
fn positions_in_mixed_currencies_are_pro_rata_in_base_units() {
    let f = setup();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    fund(&f, &f.eurc, &alice, 100_000);
    fund(&f, &f.ngnc, &bob, 100_000_000);

    // 100_000 EURC -> 108_000 base; 100_000_000 NGNC -> 100_000 base.
    f.client.invest_with_currency(
        &f.invoice_id,
        &alice,
        &100_000,
        &Symbol::new(&f.env, "EURC"),
    );
    f.client.invest_with_currency(
        &f.invoice_id,
        &bob,
        &100_000_000,
        &Symbol::new(&f.env, "NGNC"),
    );

    let alice_pos = f.client.get_investor_position(&f.invoice_id, &alice);
    let bob_pos = f.client.get_investor_position(&f.invoice_id, &bob);
    assert_eq!(alice_pos, 108_000);
    assert_eq!(bob_pos, 100_000);
    assert_eq!(
        f.client.get_invoice(&f.invoice_id).total_raised,
        alice_pos + bob_pos
    );
}

#[test]
fn setters_are_admin_only() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let eurc = Symbol::new(&f.env, "EURC");

    assert_eq!(
        f.client.try_set_supported_currency(&impostor, &eurc, &f.eurc),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_fx_oracle(&impostor, &f.eurc),
        Err(Ok(Error::Unauthorized))
    );
}
