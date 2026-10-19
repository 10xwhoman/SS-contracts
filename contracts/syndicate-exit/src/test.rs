#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_syndicate_exit_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let lead = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let token_client = token::Client::new(&env, &token_addr);
    let asset_client = token::StellarAssetClient::new(&env, &token_addr);

    let contract_id = env.register(SyndicateExitContract, ());
    let client = SyndicateExitContractClient::new(&env, &contract_id);

    // Create syndicate: target 10,000
    let syndicate_id = client.create_syndicate(&lead, &token_addr, &10_000);

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    asset_client.mint(&member1, &5000);
    asset_client.mint(&member2, &5000);

    // Commit funds
    client.commit(&member1, &syndicate_id, &3000);
    client.commit(&member2, &syndicate_id, &4000);

    let syn = client.get_syndicate(&syndicate_id).unwrap();
    assert_eq!(syn.total_committed, 7000);
    assert_eq!(syn.target_amount, 10_000);
    assert_eq!(token_client.balance(&member1), 2000);

    // Non-member exit fails
    let non_member = Address::generate(&env);
    let err = client.try_exit_syndicate(&non_member, &syndicate_id);
    assert_eq!(err, Err(Ok(Error::NotMember)));

    // Member 1 exits early
    let refunded = client.exit_syndicate(&member1, &syndicate_id);
    assert_eq!(refunded, 3000);
    assert_eq!(token_client.balance(&member1), 5000); // Fully refunded

    let syn_after = client.get_syndicate(&syndicate_id).unwrap();
    assert_eq!(syn_after.total_committed, 4000);
    // Target reduced by 3000 -> 7000
    assert_eq!(syn_after.target_amount, 7000);
    assert_eq!(client.get_commitment(&syndicate_id, &member1), 0);

    // Execute syndicate
    client.execute_syndicate(&lead, &syndicate_id);

    // Exit after execution fails with SyndicateAlreadyExecuted
    let err_after = client.try_exit_syndicate(&member2, &syndicate_id);
    assert_eq!(err_after, Err(Ok(Error::SyndicateAlreadyExecuted)));
}
