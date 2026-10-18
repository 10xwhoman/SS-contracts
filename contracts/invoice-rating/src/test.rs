use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

fn setup<'a>(env: &'a Env) -> (InvoiceRatingClient<'a>, Address, Address) {
    env.mock_all_auths();
    let client = InvoiceRatingClient::new(env, &env.register(InvoiceRating, ()));
    let admin = Address::generate(env);
    let operator = Address::generate(env);
    client.initialize(&admin, &operator);
    (client, operator, Address::generate(env))
}

fn settled_invoice(
    env: &Env,
    client: &InvoiceRatingClient,
    operator: &Address,
    issuer: &Address,
    id: u8,
    holders: &[&Address],
) -> BytesN<32> {
    let invoice_id = BytesN::from_array(env, &[id; 32]);
    client.register_invoice(operator, &invoice_id, issuer);
    for holder in holders {
        client.set_fraction_holder(operator, &invoice_id, holder, &true);
    }
    client.mark_settled(operator, &invoice_id);
    invoice_id
}

#[test]
fn rejects_non_holder_and_pre_settlement() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let stranger = Address::generate(&env);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_invoice(&operator, &invoice_id, &issuer);
    client.set_fraction_holder(&operator, &invoice_id, &holder, &true);

    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &5),
        Err(Ok(Error::InvoiceNotSettled))
    );

    client.mark_settled(&operator, &invoice_id);
    assert_eq!(
        client.try_submit_rating(&stranger, &invoice_id, &5),
        Err(Ok(Error::NotFractionHolder))
    );
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &6),
        Err(Ok(Error::InvalidScore))
    );
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &0),
        Err(Ok(Error::InvalidScore))
    );
}

#[test]
fn rejects_duplicate_rating() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&holder]);

    client.submit_rating(&holder, &invoice_id, &4);
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &5),
        Err(Ok(Error::AlreadyRated))
    );
}

#[test]
fn aggregates_invoice_ratings() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&a, &b, &c]);

    client.submit_rating(&a, &invoice_id, &5);
    client.submit_rating(&b, &invoice_id, &4);

    let partial = client.get_aggregate_rating(&invoice_id);
    assert_eq!(partial.total_ratings, 2);
    assert!(!partial.valid);

    client.submit_rating(&c, &invoice_id, &4);
    let agg = client.get_aggregate_rating(&invoice_id);
    assert_eq!(agg.total_ratings, 3);
    assert_eq!(agg.average_x100, 433);
    assert!(agg.valid);
    assert_eq!(agg.distribution.get(3).unwrap(), 2);
    assert_eq!(agg.distribution.get(4).unwrap(), 1);
    assert_eq!(agg.distribution.get(0).unwrap(), 0);
}

#[test]
fn aggregates_issuer_rating_across_invoices() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let first = settled_invoice(&env, &client, &operator, &issuer, 1, &[&a, &b]);
    let second = settled_invoice(&env, &client, &operator, &issuer, 2, &[&a]);

    client.submit_rating(&a, &first, &5);
    client.submit_rating(&b, &first, &3);
    client.submit_rating(&a, &second, &1);

    let agg = client.get_issuer_rating(&issuer);
    assert_eq!(agg.total_ratings, 3);
    assert_eq!(agg.average_x100, 300);
    assert!(agg.valid);

    // Per-invoice aggregates stay separate.
    assert_eq!(client.get_aggregate_rating(&second).total_ratings, 1);
}
