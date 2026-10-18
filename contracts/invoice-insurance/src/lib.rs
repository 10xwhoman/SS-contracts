#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{Policy, StorageKey};
use types::{BPS_DENOMINATOR, PREMIUM_BPS};

#[contract]
pub struct InvoiceInsurance;

fn get_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn get_token(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Token)
        .ok_or(Error::NotInitialized)
}

fn mul_bps(amount: i128, bps: i128) -> Result<i128, Error> {
    amount
        .checked_mul(bps)
        .map(|v| v / BPS_DENOMINATOR)
        .ok_or(Error::Overflow)
}

#[contractimpl]
impl InvoiceInsurance {
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Token, &token);
        Ok(())
    }

    /// Investor buys coverage on `principal` invested in the invoice. The
    /// premium (2% of the coverage amount) is paid upfront into the pool.
    pub fn purchase_insurance(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        principal: i128,
        coverage_bps: u32,
    ) -> Result<(), Error> {
        investor.require_auth();
        let token_addr = get_token(&env)?;

        if principal <= 0 {
            return Err(Error::InvalidAmount);
        }
        if coverage_bps == 0 || coverage_bps as i128 > BPS_DENOMINATOR {
            return Err(Error::InvalidCoverage);
        }
        if env
            .storage()
            .persistent()
            .has(&StorageKey::PayoutTriggered(invoice_id.clone()))
        {
            return Err(Error::PayoutAlreadyTriggered);
        }
        let policy_key = StorageKey::Policy(invoice_id.clone(), investor.clone());
        if env.storage().persistent().has(&policy_key) {
            return Err(Error::PolicyAlreadyExists);
        }

        let coverage_amount = mul_bps(principal, coverage_bps as i128)?;
        let premium = mul_bps(coverage_amount, PREMIUM_BPS)?;
        if premium <= 0 {
            return Err(Error::InvalidAmount);
        }

        token::Client::new(&env, &token_addr).transfer(
            &investor,
            &env.current_contract_address(),
            &premium,
        );

        env.storage().persistent().set(
            &policy_key,
            &Policy {
                investor: investor.clone(),
                principal,
                coverage_bps,
                coverage_amount,
                premium,
            },
        );
        let holders_key = StorageKey::Policyholders(invoice_id.clone());
        let mut holders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&holders_key)
            .unwrap_or(Vec::new(&env));
        holders.push_back(investor.clone());
        env.storage().persistent().set(&holders_key, &holders);

        events::insurance_purchased(&env, &invoice_id, &investor, coverage_amount, premium);
        Ok(())
    }

    /// Admin only, after the invoice default is declared. `loss_bps` is the
    /// share of principal lost (10_000 = total loss). Each policy receives
    /// `coverage_bps` of its principal loss, capped at its coverage amount.
    /// Returns the total amount paid out.
    pub fn trigger_insurance_payout(
        env: Env,
        invoice_id: BytesN<32>,
        loss_bps: u32,
    ) -> Result<i128, Error> {
        let admin = get_admin(&env)?;
        admin.require_auth();
        let token_addr = get_token(&env)?;

        if loss_bps == 0 || loss_bps as i128 > BPS_DENOMINATOR {
            return Err(Error::InvalidLoss);
        }
        let triggered_key = StorageKey::PayoutTriggered(invoice_id.clone());
        if env.storage().persistent().has(&triggered_key) {
            return Err(Error::PayoutAlreadyTriggered);
        }

        let holders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&StorageKey::Policyholders(invoice_id.clone()))
            .unwrap_or(Vec::new(&env));

        let token_client = token::Client::new(&env, &token_addr);
        let pool = env.current_contract_address();
        let mut total_paid: i128 = 0;

        for holder in holders.iter() {
            let policy: Policy = env
                .storage()
                .persistent()
                .get(&StorageKey::Policy(invoice_id.clone(), holder.clone()))
                .unwrap();

            let principal_loss = mul_bps(policy.principal, loss_bps as i128)?;
            let payout = mul_bps(principal_loss, policy.coverage_bps as i128)?
                .min(policy.coverage_amount);
            if payout <= 0 {
                continue;
            }
            if token_client.balance(&pool) < payout {
                return Err(Error::InsufficientPool);
            }
            token_client.transfer(&pool, &holder, &payout);
            total_paid += payout;

            events::insurance_payout_triggered(&env, &invoice_id, &holder, payout);
        }

        env.storage().persistent().set(&triggered_key, &true);
        Ok(total_paid)
    }

    /// View: policy of an investor on an invoice.
    pub fn get_policy(env: Env, invoice_id: BytesN<32>, investor: Address) -> Option<Policy> {
        env.storage()
            .persistent()
            .get(&StorageKey::Policy(invoice_id, investor))
    }
}
