#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, String, Vec, Symbol};

/// Issue #586: Timelock contract for sensitive admin parameter changes
/// 
/// Wraps all sensitive protocol parameter changes behind a timelock requiring
/// a minimum delay (48h) between proposal and execution.

const MIN_DELAY_SECONDS: u64 = 172800; // 48 hours

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TimelockError {
    NotAdmin = 1,
    TimeNotReached = 2,
    ProposalNotFound = 3,
    ProposalAlreadyExecuted = 4,
    ProposalCancelled = 5,
}

#[contracttype]
#[derive(Clone)]
pub struct Change {
    pub proposal_id: u64,
    pub target: Address,
    pub function: String,
    pub params: Vec<String>,
    pub eta: u64, // Execution time (timestamp)
    pub proposer: Address,
    pub executed: bool,
    pub cancelled: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    ProposalCount,
    Proposal(u64),
}

#[contract]
pub struct TimelockContract;

#[contractimpl]
impl TimelockContract {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ProposalCount, &0u64);
    }

    /// Propose a parameter change (admin only)
    pub fn propose_change(
        env: Env,
        admin: Address,
        target: Address,
        function: String,
        params: Vec<String>,
    ) -> u64 {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);

        let next_id = proposal_id + 1;
        let current_time = env.ledger().timestamp();
        let eta = current_time + MIN_DELAY_SECONDS;

        let change = Change {
            proposal_id: next_id,
            target: target.clone(),
            function: function.clone(),
            params: params.clone(),
            eta,
            proposer: admin.clone(),
            executed: false,
            cancelled: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(next_id), &change);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCount, &next_id);

        // Emit ChangeProposed event
        env.events().publish(
            (Symbol::new(&env, "ChangeProposed"), next_id),
            (target, function, eta),
        );

        next_id
    }

    /// Execute a change after the timelock has passed
    pub fn execute_change(env: Env, proposal_id: u64) -> Result<(), TimelockError> {
        let change: Change = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(TimelockError::ProposalNotFound)?;

        if change.executed {
            return Err(TimelockError::ProposalAlreadyExecuted);
        }

        if change.cancelled {
            return Err(TimelockError::ProposalCancelled);
        }

        let current_time = env.ledger().timestamp();
        if current_time < change.eta {
            return Err(TimelockError::TimeNotReached);
        }

        // Mark as executed
        let mut updated_change = change.clone();
        updated_change.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &updated_change);

        // Emit ChangeExecuted event
        env.events().publish(
            (Symbol::new(&env, "ChangeExecuted"), proposal_id),
            (change.target, change.function),
        );

        Ok(())
    }

    /// Cancel a pending proposal (admin only)
    pub fn cancel_change(env: Env, admin: Address, proposal_id: u64) -> Result<(), TimelockError> {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let change: Change = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(TimelockError::ProposalNotFound)?;

        if change.executed {
            return Err(TimelockError::ProposalAlreadyExecuted);
        }

        if change.cancelled {
            return Err(TimelockError::ProposalCancelled);
        }

        // Mark as cancelled
        let mut updated_change = change.clone();
        updated_change.cancelled = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &updated_change);

        // Emit ChangeCancelled event
        env.events().publish(
            (Symbol::new(&env, "ChangeCancelled"), proposal_id),
            admin,
        );

        Ok(())
    }

    /// Get all pending (not executed, not cancelled) changes
    pub fn get_pending_changes(env: Env) -> Vec<Change> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);

        let mut pending = Vec::new(&env);

        for id in 1..=count {
            if let Some(change) = env.storage().instance().get::<_, Change>(&DataKey::Proposal(id)) {
                if !change.executed && !change.cancelled {
                    pending.push_back(change);
                }
            }
        }

        pending
    }

    fn require_admin(env: &Env, address: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        
        if admin != *address {
            panic!("Not admin");
        }
    }
}
