#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
};

#[cfg(test)]
mod test;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    SyndicateAlreadyExecuted = 1,
    NotMember = 2,
    InvalidAmount = 3,
    SyndicateNotFound = 4,
    Unauthorized = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Syndicate {
    pub id: u32,
    pub lead: Address,
    pub token: Address,
    pub target_amount: i128,
    pub total_committed: i128,
    pub is_executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    NextSyndicateId,
    Syndicate(u32),
    MemberCommitment(u32, Address),
}

#[contract]
pub struct SyndicateExitContract;

#[contractimpl]
impl SyndicateExitContract {
    pub fn create_syndicate(
        env: Env,
        lead: Address,
        token: Address,
        target_amount: i128,
    ) -> Result<u32, Error> {
        lead.require_auth();
        if target_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextSyndicateId)
            .unwrap_or(1u32);
        env.storage()
            .instance()
            .set(&StorageKey::NextSyndicateId, &(id + 1));

        let syndicate = Syndicate {
            id,
            lead,
            token,
            target_amount,
            total_committed: 0,
            is_executed: false,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Syndicate(id), &syndicate);

        Ok(id)
    }

    pub fn commit(
        env: Env,
        member: Address,
        syndicate_id: u32,
        amount: i128,
    ) -> Result<(), Error> {
        member.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut syndicate: Syndicate = env
            .storage()
            .persistent()
            .get(&StorageKey::Syndicate(syndicate_id))
            .ok_or(Error::SyndicateNotFound)?;

        if syndicate.is_executed {
            return Err(Error::SyndicateAlreadyExecuted);
        }

        let token_client = token::Client::new(&env, &syndicate.token);
        token_client.transfer(&member, &env.current_contract_address(), &amount);

        syndicate.total_committed += amount;
        env.storage()
            .persistent()
            .set(&StorageKey::Syndicate(syndicate_id), &syndicate);

        let current_commitment: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::MemberCommitment(syndicate_id, member.clone()))
            .unwrap_or(0);
        env.storage().persistent().set(
            &StorageKey::MemberCommitment(syndicate_id, member),
            &(current_commitment + amount),
        );

        Ok(())
    }

    /// Exit syndicate early before execution.
    /// Refunds escrowed funds atomically, reduces syndicate target amount,
    /// and emits SyndicateMemberExited.
    pub fn exit_syndicate(env: Env, member: Address, syndicate_id: u32) -> Result<i128, Error> {
        member.require_auth();

        let mut syndicate: Syndicate = env
            .storage()
            .persistent()
            .get(&StorageKey::Syndicate(syndicate_id))
            .ok_or(Error::SyndicateNotFound)?;

        if syndicate.is_executed {
            return Err(Error::SyndicateAlreadyExecuted);
        }

        let commitment_key = StorageKey::MemberCommitment(syndicate_id, member.clone());
        let amount: i128 = env
            .storage()
            .persistent()
            .get(&commitment_key)
            .unwrap_or(0);

        if amount <= 0 {
            return Err(Error::NotMember);
        }

        // Refund escrowed funds atomically
        let token_client = token::Client::new(&env, &syndicate.token);
        token_client.transfer(&env.current_contract_address(), &member, &amount);

        // Adjust target amount and committed amount downward
        syndicate.total_committed = syndicate.total_committed.saturating_sub(amount);
        syndicate.target_amount = syndicate.target_amount.saturating_sub(amount);
        env.storage()
            .persistent()
            .set(&StorageKey::Syndicate(syndicate_id), &syndicate);

        // Clear commitment
        env.storage().persistent().set(&commitment_key, &0i128);

        // Emit SyndicateMemberExited event
        env.events().publish(
            (symbol_short!("syn_exit"), syndicate_id, member),
            (amount, syndicate.lead),
        );

        Ok(amount)
    }

    pub fn execute_syndicate(env: Env, lead: Address, syndicate_id: u32) -> Result<(), Error> {
        lead.require_auth();

        let mut syndicate: Syndicate = env
            .storage()
            .persistent()
            .get(&StorageKey::Syndicate(syndicate_id))
            .ok_or(Error::SyndicateNotFound)?;

        if syndicate.lead != lead {
            return Err(Error::Unauthorized);
        }
        if syndicate.is_executed {
            return Err(Error::SyndicateAlreadyExecuted);
        }

        syndicate.is_executed = true;
        env.storage()
            .persistent()
            .set(&StorageKey::Syndicate(syndicate_id), &syndicate);

        Ok(())
    }

    pub fn get_syndicate(env: Env, syndicate_id: u32) -> Option<Syndicate> {
        env.storage()
            .persistent()
            .get(&StorageKey::Syndicate(syndicate_id))
    }

    pub fn get_commitment(env: Env, syndicate_id: u32, member: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::MemberCommitment(syndicate_id, member))
            .unwrap_or(0)
    }
}
