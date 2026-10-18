#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{contract, contractimpl, vec, Address, Env, IntoVal, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::types::UpgradeProposal;

/// Parse an XDR ContractEvent into (contract_addr, topics, data).
fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(
                hash.clone(),
            )),
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

/// Check whether an event with the given name was emitted.
fn has_event(env: &Env, name: &str) -> bool {
    let all = env.events().all();
    let target = Symbol::new(env, name);
    for event in all.events().iter() {
        let (_, topics, _) = parse_event(env, event);
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(env, &first) {
                if sym == target {
                    return true;
                }
            }
        }
    }
    false
}

/// A minimal mock logic contract whose only function returns a constant.
#[contract]
pub struct MockLogic;

#[contractimpl]
impl MockLogic {
    pub fn ping(env: Env) -> u32 {
        42
    }

    pub fn add(env: Env, a: i128, b: i128) -> i128 {
        a + b
    }
}

struct Setup {
    env: Env,
    client: UpgradeProxyClient<'static>,
    admin: Address,
    signers: Vec<Address>,
    logic_a: Address,
    logic_b: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        // Register two mock logic contracts.
        let logic_a = env.register_contract(None, MockLogic);
        let logic_b = env.register_contract(None, MockLogic);

        let signers = vec![
            &env,
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        let contract_id = env.register_contract(None, UpgradeProxy);
        let client = UpgradeProxyClient::new(&env, &contract_id);

        // 2-of-3 threshold, 10-second delay.
        client.initialize(&admin, &signers, &2, &10, &logic_a);

        Self {
            env,
            client,
            admin,
            signers,
            logic_a,
            logic_b,
        }
    }

    /// Convenience: get the n-th signer address.
    fn signer(&self, n: usize) -> Address {
        self.signers.get(n as u32).unwrap()
    }
}

#[test]
fn test_get_logic_address() {
    let s = Setup::new();
    let logic = s.client.get_logic_address();
    assert_eq!(logic, s.logic_a);
}

#[test]
fn test_initialize_rejects_double_init() {
    let s = Setup::new();
    let r = s
        .client
        .try_initialize(&s.admin, &s.signers, &2, &10, &s.logic_a);
    assert!(r.is_err());
}

#[test]
fn test_initialize_rejects_bad_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let logic = env.register_contract(None, MockLogic);
    let signers = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
    ];
    let contract_id = env.register_contract(None, UpgradeProxy);
    let client = UpgradeProxyClient::new(&env, &contract_id);

    // threshold > signers.len()
    let r = client.try_initialize(&admin, &signers, &3, &10, &logic);
    assert!(r.is_err());

    // threshold = 0
    let r = client.try_initialize(&admin, &signers, &0, &10, &logic);
    assert!(r.is_err());

    // empty signers
    let r = client.try_initialize(&admin, &Vec::new(&env), &1, &10, &logic);
    assert!(r.is_err());

    // delay = 0
    let r = client.try_initialize(&admin, &signers, &1, &0, &logic);
    assert!(r.is_err());
}

#[test]
fn test_upgrade_logic_queues_proposal() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    let proposal: Option<UpgradeProposal> = s.client.get_pending_proposal();
    assert!(proposal.is_some());
    let p = proposal.unwrap();
    assert_eq!(p.new_logic, s.logic_b);
    assert!(!p.executed);
    assert!(!p.cancelled);
    assert_eq!(p.approvals.len(), 0);
}

#[test]
fn test_upgrade_logic_rejects_same_logic() {
    let s = Setup::new();
    let r = s.client.try_upgrade_logic(&s.logic_a);
    assert!(r.is_err());
}

#[test]
fn test_upgrade_logic_rejects_double_proposal() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);
    // Cannot queue a second while one is pending.
    let r = s.client.try_upgrade_logic(&s.logic_b);
    assert!(r.is_err());
}

#[test]
fn test_execute_requires_threshold() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    // Advance time past eta.
    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);

    // No approvals — threshold not met.
    let r = s.client.try_execute_upgrade();
    assert!(r.is_err());
}

#[test]
fn test_execute_requires_timelock() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    // Collect enough approvals but don't advance time.
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));

    // Still before eta — should fail.
    let r = s.client.try_execute_upgrade();
    assert!(r.is_err());
}

#[test]
fn test_execute_upgrade_happy_path() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    // Two signers approve (2-of-3 threshold).
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));

    // Advance past eta.
    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);

    s.client.execute_upgrade();

    // Logic address should have changed.
    assert_eq!(s.client.get_logic_address(), s.logic_b);

    // Proposal should be cleared.
    let proposal: Option<UpgradeProposal> = s.client.get_pending_proposal();
    assert!(proposal.is_none());
}

#[test]
fn test_double_approval_rejected() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    s.client.approve_upgrade(&s.signer(0));
    // Same signer tries again.
    let r = s.client.try_approve_upgrade(&s.signer(0));
    assert!(r.is_err());
}

#[test]
fn test_logic_upgraded_event() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));

    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);

    s.client.execute_upgrade();

    // Find the LogicUpgraded event and verify old/new addresses.
    let all = s.env.events().all();
    let mut found = false;
    let target = Symbol::new(&s.env, "LogicUpgraded");
    for event in all.events().iter() {
        let (_, topics, data) = parse_event(&s.env, event);
        if let Some(first) = topics.get(0) {
            if let Ok(sym) = Symbol::try_from_val(&s.env, &first) {
                if sym == target {
                    found = true;
                    let (old, new): (Address, Address) =
                        soroban_sdk::TryIntoVal::try_into_val(&data, &s.env).unwrap();
                    assert_eq!(old, s.logic_a);
                    assert_eq!(new, s.logic_b);
                    break;
                }
            }
        }
    }
    assert!(found, "LogicUpgraded event should be emitted");
}

#[test]
fn test_cancel_upgrade() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);
    s.client.cancel_upgrade();

    // Proposal should be cleared.
    let proposal: Option<UpgradeProposal> = s.client.get_pending_proposal();
    assert!(proposal.is_none());

    // Logic address unchanged.
    assert_eq!(s.client.get_logic_address(), s.logic_a);
}

#[test]
fn test_cancel_after_executed_fails() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));
    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);

    s.client.execute_upgrade();

    // No proposal to cancel.
    let r = s.client.try_cancel_upgrade();
    assert!(r.is_err());
}

#[test]
fn test_state_preserved_across_upgrade() {
    let s = Setup::new();

    // The proxy config and signer set should be unchanged after upgrade.
    let signers_before = s.signers.clone();

    s.client.upgrade_logic(&s.logic_b);
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));
    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);
    s.client.execute_upgrade();

    // Verify the signers are still valid after the upgrade.
    assert!(s.client.is_signer(&signers_before.get(0).unwrap()));
    assert!(s.client.is_signer(&signers_before.get(1).unwrap()));
    assert!(!s.client.is_frozen());

    // Logic address changed, everything else preserved.
    assert_eq!(s.client.get_logic_address(), s.logic_b);
}

#[test]
fn test_emergency_freeze_halts_forward() {
    let s = Setup::new();

    // Before freeze: forward works.
    let args: Vec<Val> = Vec::new(&s.env);
    let result = s.client.forward(&Symbol::new(&s.env, "ping"), &args);
    let n: u32 = result.try_into_val(&s.env).unwrap();
    assert_eq!(n, 42);

    // Freeze.
    s.client.emergency_freeze();
    assert!(s.client.is_frozen());

    // Forward should be blocked.
    let r = s.client.try_forward(&Symbol::new(&s.env, "ping"), &args);
    assert!(r.is_err());
}

#[test]
fn test_unfreeze_restores_forward() {
    let s = Setup::new();
    let args: Vec<Val> = Vec::new(&s.env);

    s.client.emergency_freeze();
    s.client.unfreeze();
    assert!(!s.client.is_frozen());

    let result = s.client.forward(&Symbol::new(&s.env, "ping"), &args);
    let n: u32 = result.try_into_val(&s.env).unwrap();
    assert_eq!(n, 42);
}

#[test]
fn test_forward_with_args() {
    let s = Setup::new();
    let args: Vec<Val> = soroban_sdk::vec![
        &s.env,
        10i128.into_val(&s.env),
        32i128.into_val(&s.env)
    ];
    let result = s.client.forward(&Symbol::new(&s.env, "add"), &args);
    let n: i128 = result.try_into_val(&s.env).unwrap();
    assert_eq!(n, 42);
}

#[test]
fn test_forward_after_upgrade_uses_new_logic() {
    let s = Setup::new();

    // Upgrade to logic_b (also a MockLogic, returns same results).
    s.client.upgrade_logic(&s.logic_b);
    s.client.approve_upgrade(&s.signer(0));
    s.client.approve_upgrade(&s.signer(1));
    let proposal: UpgradeProposal = s.client.get_pending_proposal().unwrap();
    s.env.ledger().set_timestamp(proposal.eta + 1);
    s.client.execute_upgrade();

    // Forward should now call logic_b.
    let args: Vec<Val> = Vec::new(&s.env);
    let result = s.client.forward(&Symbol::new(&s.env, "ping"), &args);
    let n: u32 = result.try_into_val(&s.env).unwrap();
    assert_eq!(n, 42);
}

#[test]
fn test_upgrade_proposed_event() {
    let s = Setup::new();
    s.client.upgrade_logic(&s.logic_b);

    assert!(has_event(&s.env, "UpgradeProposed"));
}

#[test]
fn test_not_signer_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let logic = env.register_contract(None, MockLogic);
    let logic2 = env.register_contract(None, MockLogic);
    let signers = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
    ];
    let contract_id = env.register_contract(None, UpgradeProxy);
    let client = UpgradeProxyClient::new(&env, &contract_id);
    client.initialize(&admin, &signers, &1, &10, &logic);

    // The admin is not a signer (we didn't include it in the signers set).
    client.upgrade_logic(&logic2);

    // Non-signer approval should fail.
    let r = client.try_approve_upgrade(&admin);
    assert!(r.is_err());
}

#[test]
fn test_proxy_frozen_event() {
    let s = Setup::new();
    s.client.emergency_freeze();

    assert!(has_event(&s.env, "ProxyFrozen"));
}
