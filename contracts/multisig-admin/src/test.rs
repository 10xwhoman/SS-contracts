//! Tests for the multi-sig admin contract (issue #524).

#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::{ContractEvent, ContractEventBody, ScAddress, ScVal},
    Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

const START: u64 = 5_000;

struct Ctx<'a> {
    admin: Address,
    signer_b: Address,
    signer_c: Address,
    outsider: Address,
    ms: MultisigAdminClient<'a>,
}

fn setup_with_threshold<'a>(env: &'a Env, threshold: u32) -> Ctx<'a> {
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(env);
    let signer_b = Address::generate(env);
    let signer_c = Address::generate(env);
    let outsider = Address::generate(env);

    let contract_id = env.register(MultisigAdmin, ());
    let ms = MultisigAdminClient::new(env, &contract_id);

    let mut signers = Vec::new(env);
    signers.push_back(admin.clone());
    signers.push_back(signer_b.clone());
    signers.push_back(signer_c.clone());
    ms.initialize(&admin, &signers, &threshold);

    Ctx {
        admin,
        signer_b,
        signer_c,
        outsider,
        ms,
    }
}

fn setup<'a>(env: &'a Env) -> Ctx<'a> {
    setup_with_threshold(env, 2)
}

fn empty_params() -> ActionParams {
    ActionParams {
        account: None,
        invoice_id: None,
        amount: None,
        allowed: None,
        new_signers: None,
        new_threshold: None,
    }
}

fn fee_params(bps: i128) -> ActionParams {
    let mut params = empty_params();
    params.amount = Some(bps);
    params
}

fn whitelist_params(account: &Address, allowed: bool) -> ActionParams {
    let mut params = empty_params();
    params.account = Some(account.clone());
    params.allowed = Some(allowed);
    params
}

fn settlement_params(invoice_id: &Symbol) -> ActionParams {
    let mut params = empty_params();
    params.invoice_id = Some(invoice_id.clone());
    params
}

fn update_signers_params(env: &Env, signers: &[&Address], threshold: u32) -> ActionParams {
    let mut list = Vec::new(env);
    for signer in signers {
        list.push_back((*signer).clone());
    }
    let mut params = empty_params();
    params.new_signers = Some(list);
    params.new_threshold = Some(threshold);
    params
}

fn parse_event(env: &Env, event: &ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => {
            Address::try_from_val(env, &ScVal::Address(ScAddress::Contract(hash.clone()))).unwrap()
        }
        None => Address::generate(env),
    };
    let ContractEventBody::V0(v0) = &event.body;
    let topics =
        Vec::<Val>::try_from_val(env, &ScVal::Vec(Some(v0.topics.clone().into()))).unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

fn topic_of(env: &Env, event: &ContractEvent) -> Symbol {
    let (_, topics, _) = parse_event(env, event);
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

// ── Initialization ────────────────────────────────────────────────────────────

#[test]
fn initialize_sets_signers_and_threshold() {
    let env = Env::default();
    let ctx = setup(&env);

    assert_eq!(ctx.ms.get_signers().len(), 3);
    assert_eq!(ctx.ms.get_threshold(), 2);
    assert!(ctx.ms.is_signer(&ctx.admin));
    assert!(ctx.ms.is_signer(&ctx.signer_b));
    assert!(ctx.ms.is_signer(&ctx.signer_c));
    assert!(!ctx.ms.is_signer(&ctx.outsider));
    assert!(!ctx.ms.is_paused());
    assert_eq!(ctx.ms.get_fee_bps(), 0);
}

#[test]
fn initialize_rejects_invalid_signer_sets_and_thresholds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let contract_id = env.register(MultisigAdmin, ());
    let ms = MultisigAdminClient::new(&env, &contract_id);

    let empty: Vec<Address> = Vec::new(&env);
    assert_eq!(
        ms.try_initialize(&admin, &empty, &1),
        Err(Ok(Error::InvalidSignerSet))
    );

    let mut three = Vec::new(&env);
    three.push_back(admin.clone());
    three.push_back(b.clone());
    three.push_back(c.clone());

    // Threshold 0 and threshold above signer count are invalid.
    assert_eq!(
        ms.try_initialize(&admin, &three, &0),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        ms.try_initialize(&admin, &three, &4),
        Err(Ok(Error::InvalidThreshold))
    );

    // Duplicate signers are invalid.
    let mut dupes = Vec::new(&env);
    dupes.push_back(admin.clone());
    dupes.push_back(admin.clone());
    assert_eq!(
        ms.try_initialize(&admin, &dupes, &1),
        Err(Ok(Error::InvalidSignerSet))
    );

    // Admin must be part of the initial set.
    let mut others = Vec::new(&env);
    others.push_back(b.clone());
    others.push_back(c.clone());
    assert_eq!(
        ms.try_initialize(&admin, &others, &1),
        Err(Ok(Error::AdminNotSigner))
    );

    // Valid bootstrap, then double init rejected.
    ms.initialize(&admin, &three, &2);
    assert_eq!(
        ms.try_initialize(&admin, &three, &2),
        Err(Ok(Error::AlreadyInit))
    );
}

// ── Access control ────────────────────────────────────────────────────────────

#[test]
fn unauthorized_signers_cannot_propose_or_sign() {
    let env = Env::default();
    let ctx = setup(&env);

    let result = ctx
        .ms
        .try_propose_action(&ctx.outsider, &ActionType::FeeChange, &fee_params(100));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(100));
    let result = ctx.ms.try_sign_proposal(&ctx.outsider, &proposal_id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    let result = ctx.ms.try_execute_proposal(&ctx.outsider, &proposal_id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    let result = ctx.ms.try_reject_proposal(&ctx.outsider, &proposal_id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn already_signed_and_unknown_proposal_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(100));
    // The proposer counts as the first approval.
    assert_eq!(
        ctx.ms.try_sign_proposal(&ctx.admin, &proposal_id),
        Err(Ok(Error::AlreadySigned))
    );

    assert_eq!(
        ctx.ms.try_get_proposal(&99),
        Err(Ok(Error::ProposalNotFound))
    );
    assert_eq!(
        ctx.ms.try_sign_proposal(&ctx.admin, &99),
        Err(Ok(Error::ProposalNotFound))
    );
    assert_eq!(
        ctx.ms.try_execute_proposal(&ctx.admin, &99),
        Err(Ok(Error::ProposalNotFound))
    );
    assert_eq!(
        ctx.ms.try_reject_proposal(&ctx.admin, &99),
        Err(Ok(Error::ProposalNotFound))
    );
}

// ── Threshold enforcement ─────────────────────────────────────────────────────

#[test]
fn execution_blocked_until_threshold_reached() {
    let env = Env::default();
    let ctx = setup(&env);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(300));
    assert_eq!(proposal_id, 1);

    // One approval (the proposer) is not enough for a threshold of 2.
    assert_eq!(
        ctx.ms.try_execute_proposal(&ctx.admin, &proposal_id),
        Err(Ok(Error::ThresholdNotReached))
    );
    assert_eq!(ctx.ms.get_fee_bps(), 0);

    // The second approval satisfies the threshold.
    ctx.ms.sign_proposal(&ctx.signer_b, &proposal_id);
    ctx.ms.execute_proposal(&ctx.signer_b, &proposal_id);

    assert_eq!(ctx.ms.get_fee_bps(), 300);
    let proposal = ctx.ms.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
    assert_eq!(proposal.approvals.len(), 2);
    assert_eq!(proposal.executed_at, START);

    // Executed proposals cannot be signed or executed again.
    let result = ctx.ms.try_sign_proposal(&ctx.signer_c, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalNotPending)));
    let result = ctx.ms.try_execute_proposal(&ctx.signer_c, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalNotPending)));
}

#[test]
fn threshold_of_one_allows_direct_execution() {
    let env = Env::default();
    let ctx = setup_with_threshold(&env, 1);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(50));
    ctx.ms.execute_proposal(&ctx.admin, &proposal_id);
    assert_eq!(ctx.ms.get_fee_bps(), 50);
}

// ── Rejection ─────────────────────────────────────────────────────────────────

#[test]
fn reject_prevents_execution_even_after_threshold_reached() {
    let env = Env::default();
    let ctx = setup(&env);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(400));

    ctx.ms.reject_proposal(&ctx.signer_b, &proposal_id);

    // Further approvals may still be collected (threshold is reached below),
    // but execution stays blocked forever.
    ctx.ms.sign_proposal(&ctx.signer_c, &proposal_id);
    let proposal = ctx.ms.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 2);
    assert_eq!(proposal.status, ProposalStatus::Rejected);
    assert_eq!(proposal.rejected_by, Some(ctx.signer_b.clone()));

    let result = ctx.ms.try_execute_proposal(&ctx.signer_c, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalWasRejected)));
    let result = ctx.ms.try_execute_proposal(&ctx.admin, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalWasRejected)));

    // The action never applied.
    assert_eq!(ctx.ms.get_fee_bps(), 0);
}

#[test]
fn reject_requires_signer_and_pending_proposal() {
    let env = Env::default();
    let ctx = setup(&env);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(100));
    ctx.ms.sign_proposal(&ctx.signer_b, &proposal_id);
    ctx.ms.execute_proposal(&ctx.signer_b, &proposal_id);

    // Executed proposals cannot be rejected.
    let result = ctx.ms.try_reject_proposal(&ctx.signer_c, &proposal_id);
    assert_eq!(result, Err(Ok(Error::ProposalNotPending)));
}

// ── Signer set updates ────────────────────────────────────────────────────────

#[test]
fn signer_set_update_requires_current_threshold_approval() {
    let env = Env::default();
    let ctx = setup(&env);
    let d = Address::generate(&env);
    let e = Address::generate(&env);

    let proposal_id = ctx.ms.propose_action(
        &ctx.admin,
        &ActionType::UpdateSigners,
        &update_signers_params(&env, &[&d, &e], 2),
    );

    // Only the proposer has approved so far; the current threshold is 2.
    assert_eq!(
        ctx.ms.try_execute_proposal(&ctx.admin, &proposal_id),
        Err(Ok(Error::ThresholdNotReached))
    );
    assert_eq!(ctx.ms.get_signers().len(), 3);

    ctx.ms.sign_proposal(&ctx.signer_b, &proposal_id);
    ctx.ms.execute_proposal(&ctx.signer_b, &proposal_id);

    // Capture the update event from the execute invocation before any further
    // contract calls (event readers only see the last invocation).
    let mut found = false;
    for event in env.events().all().events().iter() {
        if topic_of(&env, event) == Symbol::new(&env, "signers_updated") {
            let (_, _, data) = parse_event(&env, event);
            let (id, count, threshold): (u32, u32, u32) = data.try_into_val(&env).unwrap();
            assert_eq!(id, proposal_id);
            assert_eq!(count, 2);
            assert_eq!(threshold, 2);
            found = true;
        }
    }
    assert!(found, "expected SignersUpdated event");

    // Signer set and threshold replaced.
    let signers = ctx.ms.get_signers();
    assert_eq!(signers.len(), 2);
    assert_eq!(signers.get(0).unwrap(), d);
    assert_eq!(signers.get(1).unwrap(), e);
    assert_eq!(ctx.ms.get_threshold(), 2);
    assert!(!ctx.ms.is_signer(&ctx.admin));
    assert!(ctx.ms.is_signer(&d));

    // Removed signers can no longer propose.
    assert_eq!(
        ctx.ms
            .try_propose_action(&ctx.admin, &ActionType::Pause, &empty_params()),
        Err(Ok(Error::Unauthorized))
    );
    // New signers can.
    let next_id = ctx
        .ms
        .propose_action(&d, &ActionType::Pause, &empty_params());
    assert_eq!(next_id, 2);
}

#[test]
fn stale_approvals_from_removed_signers_do_not_count() {
    let env = Env::default();
    let ctx = setup(&env);
    let d = Address::generate(&env);
    let e = Address::generate(&env);

    // Pending fee proposal approved only by the admin.
    let fee_proposal = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(250));

    // Replace the signer set with {d, e} (threshold 2).
    let update_proposal = ctx.ms.propose_action(
        &ctx.admin,
        &ActionType::UpdateSigners,
        &update_signers_params(&env, &[&d, &e], 2),
    );
    ctx.ms.sign_proposal(&ctx.signer_b, &update_proposal);
    ctx.ms.execute_proposal(&ctx.signer_b, &update_proposal);

    // The admin's stale approval no longer counts toward the threshold.
    assert_eq!(
        ctx.ms.try_execute_proposal(&d, &fee_proposal),
        Err(Ok(Error::ThresholdNotReached))
    );

    ctx.ms.sign_proposal(&d, &fee_proposal);
    // One valid approval is still short of the threshold of 2.
    assert_eq!(
        ctx.ms.try_execute_proposal(&e, &fee_proposal),
        Err(Ok(Error::ThresholdNotReached))
    );

    ctx.ms.sign_proposal(&e, &fee_proposal);
    ctx.ms.execute_proposal(&e, &fee_proposal);
    assert_eq!(ctx.ms.get_fee_bps(), 250);
}

// ── Action effects ────────────────────────────────────────────────────────────

#[test]
fn whitelist_action_adds_and_removes_accounts() {
    let env = Env::default();
    let ctx = setup_with_threshold(&env, 1);

    let add = ctx.ms.propose_action(
        &ctx.admin,
        &ActionType::WhitelistUpdate,
        &whitelist_params(&ctx.outsider, true),
    );
    ctx.ms.execute_proposal(&ctx.admin, &add);
    assert!(ctx.ms.is_whitelisted(&ctx.outsider));

    let remove = ctx.ms.propose_action(
        &ctx.admin,
        &ActionType::WhitelistUpdate,
        &whitelist_params(&ctx.outsider, false),
    );
    ctx.ms.execute_proposal(&ctx.admin, &remove);
    assert!(!ctx.ms.is_whitelisted(&ctx.outsider));
}

#[test]
fn pause_and_resume_actions_toggle_flag() {
    let env = Env::default();
    let ctx = setup_with_threshold(&env, 1);

    let pause = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::Pause, &empty_params());
    ctx.ms.execute_proposal(&ctx.admin, &pause);
    assert!(ctx.ms.is_paused());

    let resume = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::Resume, &empty_params());
    ctx.ms.execute_proposal(&ctx.admin, &resume);
    assert!(!ctx.ms.is_paused());
}

#[test]
fn settlement_trigger_action_records_invoice() {
    let env = Env::default();
    let ctx = setup_with_threshold(&env, 1);
    let invoice_id = Symbol::new(&env, "INVSETTLE");

    assert!(!ctx.ms.is_settlement_triggered(&invoice_id));
    let proposal_id = ctx.ms.propose_action(
        &ctx.admin,
        &ActionType::SettlementTrigger,
        &settlement_params(&invoice_id),
    );
    ctx.ms.execute_proposal(&ctx.admin, &proposal_id);
    assert!(ctx.ms.is_settlement_triggered(&invoice_id));
}

#[test]
fn invalid_action_params_rejected_at_proposal() {
    let env = Env::default();
    let ctx = setup(&env);
    let d = Address::generate(&env);

    // Whitelist update without the account field.
    let mut wl = empty_params();
    wl.allowed = Some(true);
    assert_eq!(
        ctx.ms
            .try_propose_action(&ctx.admin, &ActionType::WhitelistUpdate, &wl),
        Err(Ok(Error::InvalidActionParams))
    );

    // Fee out of range.
    assert_eq!(
        ctx.ms
            .try_propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(10_001)),
        Err(Ok(Error::InvalidActionParams))
    );
    assert_eq!(
        ctx.ms
            .try_propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(-1)),
        Err(Ok(Error::InvalidActionParams))
    );

    // Settlement trigger without an invoice id.
    assert_eq!(
        ctx.ms
            .try_propose_action(&ctx.admin, &ActionType::SettlementTrigger, &empty_params()),
        Err(Ok(Error::InvalidActionParams))
    );

    // Signer update with an invalid threshold or duplicates.
    assert_eq!(
        ctx.ms.try_propose_action(
            &ctx.admin,
            &ActionType::UpdateSigners,
            &update_signers_params(&env, &[&d], 0)
        ),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        ctx.ms.try_propose_action(
            &ctx.admin,
            &ActionType::UpdateSigners,
            &update_signers_params(&env, &[&d, &d], 1)
        ),
        Err(Ok(Error::InvalidSignerSet))
    );

    // Pause needs no parameters.
    assert!(ctx
        .ms
        .try_propose_action(&ctx.admin, &ActionType::Pause, &empty_params())
        .is_ok());
}

// ── Events ────────────────────────────────────────────────────────────────────

#[test]
fn events_carry_proposal_id_and_actor() {
    let env = Env::default();
    let ctx = setup(&env);

    let mut proposed = false;
    let mut signed = false;
    let mut executed = false;
    let mut rejected = false;

    // `env.events().all()` returns only the events of the most recent contract
    // invocation, so each event is captured right after its emitting call.
    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(300));
    for event in env.events().all().events().iter() {
        if topic_of(&env, event) == Symbol::new(&env, "action_proposed") {
            let (_, _, data) = parse_event(&env, event);
            let (id, actor, action): (u32, Address, ActionType) = data.try_into_val(&env).unwrap();
            assert_eq!(id, proposal_id);
            assert_eq!(actor, ctx.admin);
            assert_eq!(action, ActionType::FeeChange);
            proposed = true;
        }
    }

    ctx.ms.sign_proposal(&ctx.signer_b, &proposal_id);
    for event in env.events().all().events().iter() {
        if topic_of(&env, event) == Symbol::new(&env, "proposal_signed") {
            let (_, _, data) = parse_event(&env, event);
            let (id, actor, count): (u32, Address, u32) = data.try_into_val(&env).unwrap();
            assert_eq!(id, proposal_id);
            assert_eq!(actor, ctx.signer_b);
            assert_eq!(count, 2);
            signed = true;
        }
    }

    ctx.ms.execute_proposal(&ctx.signer_b, &proposal_id);
    for event in env.events().all().events().iter() {
        if topic_of(&env, event) == Symbol::new(&env, "proposal_executed") {
            let (_, _, data) = parse_event(&env, event);
            let (id, actor, action): (u32, Address, ActionType) = data.try_into_val(&env).unwrap();
            assert_eq!(id, proposal_id);
            assert_eq!(actor, ctx.signer_b);
            assert_eq!(action, ActionType::FeeChange);
            executed = true;
        }
    }

    let rejected_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(100));
    ctx.ms.reject_proposal(&ctx.signer_c, &rejected_id);
    for event in env.events().all().events().iter() {
        if topic_of(&env, event) == Symbol::new(&env, "proposal_rejected") {
            let (_, _, data) = parse_event(&env, event);
            let (id, actor): (u32, Address) = data.try_into_val(&env).unwrap();
            assert_eq!(id, rejected_id);
            assert_eq!(actor, ctx.signer_c);
            rejected = true;
        }
    }

    assert!(proposed && signed && executed && rejected);
}

// ── Proposal bookkeeping ──────────────────────────────────────────────────────

#[test]
fn proposal_count_and_fields_track_created_proposals() {
    let env = Env::default();
    let ctx = setup(&env);
    assert_eq!(ctx.ms.proposal_count(), 0);

    let proposal_id = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::FeeChange, &fee_params(200));
    assert_eq!(ctx.ms.proposal_count(), 1);

    let proposal = ctx.ms.get_proposal(&proposal_id);
    assert_eq!(proposal.id, proposal_id);
    assert_eq!(proposal.proposer, ctx.admin);
    assert_eq!(proposal.action_type, ActionType::FeeChange);
    assert_eq!(proposal.params.amount, Some(200));
    assert_eq!(proposal.approvals.len(), 1);
    assert_eq!(proposal.rejected_by, None);
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.created_at, START);
    assert_eq!(proposal.executed_at, 0);

    // Ids are sequential.
    let second = ctx
        .ms
        .propose_action(&ctx.admin, &ActionType::Pause, &empty_params());
    assert_eq!(second, proposal_id + 1);
    assert_eq!(ctx.ms.proposal_count(), 2);
}
