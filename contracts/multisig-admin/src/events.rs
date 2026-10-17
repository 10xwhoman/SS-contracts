#![allow(deprecated)]
//! Event definitions for the multi-sig admin contract.
//!
//! Every event carries the proposal id and the acting address so indexers can
//! reconstruct the full approval trail.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::ActionType;

/// Emitted when a signer creates a proposal.
pub fn action_proposed(env: &Env, proposal_id: u32, proposer: &Address, action_type: ActionType) {
    env.events().publish(
        (Symbol::new(env, "action_proposed"),),
        (proposal_id, proposer.clone(), action_type),
    );
}

/// Emitted when a signer approves a proposal.
pub fn proposal_signed(env: &Env, proposal_id: u32, signer: &Address, approval_count: u32) {
    env.events().publish(
        (Symbol::new(env, "proposal_signed"),),
        (proposal_id, signer.clone(), approval_count),
    );
}

/// Emitted when a proposal's action is executed.
pub fn proposal_executed(env: &Env, proposal_id: u32, executor: &Address, action_type: ActionType) {
    env.events().publish(
        (Symbol::new(env, "proposal_executed"),),
        (proposal_id, executor.clone(), action_type),
    );
}

/// Emitted when a signer rejects a proposal.
pub fn proposal_rejected(env: &Env, proposal_id: u32, rejecter: &Address) {
    env.events().publish(
        (Symbol::new(env, "proposal_rejected"),),
        (proposal_id, rejecter.clone()),
    );
}

/// Emitted when an `UpdateSigners` proposal executes.
pub fn signers_updated(env: &Env, proposal_id: u32, signer_count: u32, threshold: u32) {
    env.events().publish(
        (Symbol::new(env, "signers_updated"),),
        (proposal_id, signer_count, threshold),
    );
}
