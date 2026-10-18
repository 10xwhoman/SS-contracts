use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, BytesN, Env,
};

const DAY: u64 = 86_400;
const START: u64 = 1_000_000;

struct Fixture<'a> {
    client: GraceInterestClient<'a>,
    token: Address,
    id: BytesN<32>,
    holder_a: Address,
    holder_b: Address,
}

fn setup(env: &Env) -> Fixture<'_> {
    env.mock_all_auths();
    env.ledger().set_timestamp(START - DAY);

    let contract_id = env.register(GraceInterest, ());
    let client = GraceInterestClient::new(env, &contract_id);
    client.initialize(&Address::generate(env));

    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let id = BytesN::from_array(env, &[1u8; 32]);
    let holder_a = Address::generate(env);
    let holder_b = Address::generate(env);
    let holdings = vec![
        env,
        Holding {
            holder: holder_a.clone(),
            fractions: 1,
        },
        Holding {
            holder: holder_b.clone(),
            fractions: 3,
        },
    ];
    client.register_invoice(&id, &token, &100_000, &START, &holdings);
    Fixture {
        client,
        token,
        id,
        holder_a,
        holder_b,
    }
}

#[test]
fn test_accrual_blocked_before_grace_period() {
    let env = Env::default();
    let f = setup(&env);
    let res = f.client.try_accrue_interest(&f.id);
    assert_eq!(res, Err(Ok(Error::GracePeriodNotStarted)));

    env.ledger().set_timestamp(START + DAY - 1);
    let res = f.client.try_accrue_interest(&f.id);
    assert_eq!(res, Err(Ok(Error::NothingToAccrue)));
}

#[test]
fn test_interest_accrues_daily_at_015_percent() {
    let env = Env::default();
    let f = setup(&env);

    // 0.15% of 100_000 = 150 per day.
    env.ledger().set_timestamp(START + DAY);
    assert_eq!(f.client.accrue_interest(&f.id), 150);
    assert_eq!(f.client.get_accrued_interest(&f.id), 150);

    let res = f.client.try_accrue_interest(&f.id);
    assert_eq!(res, Err(Ok(Error::NothingToAccrue)));

    // A catch-up call after a gap accrues every missed day.
    env.ledger().set_timestamp(START + 3 * DAY + 10);
    assert_eq!(f.client.accrue_interest(&f.id), 300);
    assert_eq!(f.client.get_accrued_interest(&f.id), 450);
}

#[test]
fn test_interest_distributed_pro_rata_on_settlement() {
    let env = Env::default();
    let f = setup(&env);
    env.ledger().set_timestamp(START + 2 * DAY);
    f.client.accrue_interest(&f.id);

    let payer = Address::generate(&env);
    StellarAssetClient::new(&env, &f.token).mint(&payer, &300);
    f.client.settle_interest(&f.id, &payer);

    let token = TokenClient::new(&env, &f.token);
    assert_eq!(token.balance(&f.holder_a), 75);
    assert_eq!(token.balance(&f.holder_b), 225);
    assert_eq!(token.balance(&payer), 0);

    let res = f.client.try_settle_interest(&f.id, &payer);
    assert_eq!(res, Err(Ok(Error::AlreadySettled)));
    let res = f.client.try_accrue_interest(&f.id);
    assert_eq!(res, Err(Ok(Error::AlreadySettled)));
}
