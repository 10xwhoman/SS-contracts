use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    vec, Address, BytesN, Env, String,
};

const DAY: u64 = 86_400;
const MATURITY: u64 = 100 * DAY;

struct Fixture<'a> {
    client: InvoiceExtensionClient<'a>,
    id: BytesN<32>,
    holder_a: Address,
    holder_b: Address,
}

fn setup(env: &Env) -> Fixture<'_> {
    env.mock_all_auths();
    env.ledger().set_timestamp(10 * DAY);

    let contract_id = env.register(InvoiceExtension, ());
    let client = InvoiceExtensionClient::new(env, &contract_id);
    client.initialize(&Address::generate(env));

    let id = BytesN::from_array(env, &[1u8; 32]);
    let holder_a = Address::generate(env);
    let holder_b = Address::generate(env);
    let holdings = vec![
        env,
        Holding {
            holder: holder_a.clone(),
            fractions: 60,
        },
        Holding {
            holder: holder_b.clone(),
            fractions: 40,
        },
    ];
    client.register_invoice(&id, &Address::generate(env), &MATURITY, &holdings);
    Fixture {
        client,
        id,
        holder_a,
        holder_b,
    }
}

fn reason(env: &Env) -> String {
    String::from_str(env, "customer payment delayed")
}

#[test]
fn test_extension_passes_with_majority_and_updates_maturity() {
    let env = Env::default();
    let f = setup(&env);
    let new_maturity = MATURITY + 30 * DAY;

    let pid = f.client.propose_extension(&f.id, &new_maturity, &reason(&env));
    assert_eq!(env.events().all().events().len(), 1);

    f.client.vote_extension(&pid, &f.holder_a, &true);
    f.client.vote_extension(&pid, &f.holder_b, &false);
    assert_eq!(env.events().all().events().len(), 1);
    let proposal = f.client.get_proposal(&pid);
    assert_eq!(proposal.votes_for, 60);
    assert_eq!(proposal.votes_against, 40);

    f.client.execute_extension(&pid);
    assert_eq!(env.events().all().events().len(), 1);
    assert_eq!(f.client.get_maturity(&f.id), new_maturity);

    let res = f.client.try_execute_extension(&pid);
    assert_eq!(res, Err(Ok(Error::AlreadyExecuted)));
}

#[test]
fn test_execute_reverts_without_majority() {
    let env = Env::default();
    let f = setup(&env);
    let pid = f
        .client
        .propose_extension(&f.id, &(MATURITY + 10 * DAY), &reason(&env));

    // 40 of 100 fractions is below the 51% threshold.
    f.client.vote_extension(&pid, &f.holder_b, &true);
    let res = f.client.try_vote_extension(&pid, &f.holder_b, &true);
    assert_eq!(res, Err(Ok(Error::AlreadyVoted)));
    let res = f.client.try_execute_extension(&pid);
    assert_eq!(res, Err(Ok(Error::ApprovalNotMet)));
    assert_eq!(f.client.get_maturity(&f.id), MATURITY);
}

#[test]
fn test_extension_limits_and_due_date_enforced() {
    let env = Env::default();
    let f = setup(&env);

    f.client
        .propose_extension(&f.id, &(MATURITY + 90 * DAY), &reason(&env));
    let res = f
        .client
        .try_propose_extension(&f.id, &(MATURITY + 90 * DAY + 1), &reason(&env));
    assert_eq!(res, Err(Ok(Error::MaxExtensionExceeded)));
    let res = f.client.try_propose_extension(&f.id, &MATURITY, &reason(&env));
    assert_eq!(res, Err(Ok(Error::InvalidMaturity)));

    env.ledger().set_timestamp(MATURITY);
    let res = f
        .client
        .try_propose_extension(&f.id, &(MATURITY + DAY), &reason(&env));
    assert_eq!(res, Err(Ok(Error::DueDatePassed)));
}
