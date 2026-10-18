//! Secondary-market buy limit tests (#547).

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

struct Market<'a> {
    env: Env,
    client: InvoiceEscrowClient<'a>,
    contract_id: Address,
    admin: Address,
    invoice_id: BytesN<32>,
}

fn setup<'a>() -> Market<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &300);
    let invoice_id = BytesN::from_array(&env, &[47u8; 32]);
    env.as_contract(&contract_id, || {
        let invoice = FundingInvoice {
            seller: Address::generate(&env),
            funding_target: 1_000,
            total_raised: 1_000,
            deadline_ledger: 1_000,
            min_investment: 0,
            per_investor_cap: None,
            status: InvoiceStatus::Funded,
            token: Address::generate(&env),
        };
        storage::set_invoice(&env, invoice_id.clone(), &invoice);
    });
    Market { env, client, contract_id, admin, invoice_id }
}

impl Market<'_> {
    /// A fresh seller holding `amount` of the invoice.
    fn seller(&self, amount: i128) -> Address {
        let seller = Address::generate(&self.env);
        self.env.as_contract(&self.contract_id, || {
            storage::set_investor_position(&self.env, &self.invoice_id, &seller, amount);
        });
        seller
    }

    /// Buy a whole `amount` position at zero price (no token movement needed).
    fn buy(&self, buyer: &Address, amount: i128) -> Result<(), Error> {
        let seller = self.seller(amount);
        match self.client.try_transfer_position(&seller, &self.invoice_id, buyer, &0) {
            Ok(_) => Ok(()),
            Err(Ok(err)) => Err(err),
            Err(Err(_)) => panic!("unexpected host error"),
        }
    }
}

#[test]
fn buys_within_the_limit_succeed_and_excess_is_blocked() {
    let m = setup();
    let buyer = Address::generate(&m.env);
    assert_eq!(m.client.get_buy_remaining(&m.invoice_id, &buyer), i128::MAX);
    m.client.set_buy_limit(&m.admin, &m.invoice_id, &500, &3_600);

    assert_eq!(m.buy(&buyer, 150), Ok(()));
    assert_eq!(m.buy(&buyer, 250), Ok(()));

    assert_eq!(m.buy(&buyer, 101), Err(Error::BuyLimitExceeded));
    assert_eq!(m.client.get_buy_remaining(&m.invoice_id, &buyer), 100);
    assert_eq!(m.client.get_investor_position(&m.invoice_id, &buyer), 400);
}

#[test]
fn window_resets_after_expiry() {
    let m = setup();
    let buyer = Address::generate(&m.env);
    m.client.set_buy_limit(&m.admin, &m.invoice_id, &500, &3_600);
    m.buy(&buyer, 500).unwrap();

    m.env.ledger().with_mut(|l| l.timestamp += 3_599);
    assert_eq!(m.buy(&buyer, 1), Err(Error::BuyLimitExceeded));

    m.env.ledger().with_mut(|l| l.timestamp += 1);
    assert_eq!(m.client.get_buy_remaining(&m.invoice_id, &buyer), 500);
    assert_eq!(m.buy(&buyer, 500), Ok(()));
}

#[test]
fn set_buy_limit_is_admin_only_and_validated() {
    let m = setup();
    let stranger = Address::generate(&m.env);

    assert_eq!(
        m.client.try_set_buy_limit(&stranger, &m.invoice_id, &500, &3_600),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        m.client.try_set_buy_limit(&m.admin, &m.invoice_id, &0, &3_600),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        m.client.try_set_buy_limit(&m.admin, &m.invoice_id, &500, &0),
        Err(Ok(Error::InvalidAmount))
    );
}
