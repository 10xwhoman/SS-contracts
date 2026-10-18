#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env};
pub use types::{AutoInvestCriteria, AutoInvestRule, InvoiceListing, StorageKey};

#[contract]
pub struct AutoInvest;

#[contractimpl]
impl AutoInvest {
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Operator, &operator);
        Ok(())
    }

    pub fn set_operator(env: Env, new_operator: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&StorageKey::Operator, &new_operator);
        Ok(())
    }

    pub fn set_auto_invest_rule(
        env: Env,
        wallet: Address,
        criteria: AutoInvestCriteria,
        max_amount_per_invoice: i128,
        total_budget: Option<i128>,
    ) -> Result<(), Error> {
        wallet.require_auth();

        if max_amount_per_invoice <= 0 {
            return Err(Error::InvalidAmount);
        }

        let budget = total_budget.unwrap_or(i128::MAX);
        if budget < max_amount_per_invoice {
            return Err(Error::InvalidAmount);
        }

        let rule = AutoInvestRule {
            wallet: wallet.clone(),
            criteria,
            max_amount_per_invoice,
            total_budget: budget,
            total_invested: 0,
            active: true,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Rule(wallet.clone()), &rule);

        events::auto_invest_rule_updated(&env, &wallet, max_amount_per_invoice, budget, true);

        Ok(())
    }

    pub fn disable_auto_invest(env: Env, wallet: Address) -> Result<(), Error> {
        wallet.require_auth();

        let mut rule: AutoInvestRule = env
            .storage()
            .persistent()
            .get(&StorageKey::Rule(wallet.clone()))
            .ok_or(Error::RuleNotFound)?;

        rule.active = false;
        env.storage()
            .persistent()
            .set(&StorageKey::Rule(wallet.clone()), &rule);

        events::auto_invest_disabled(&env, &wallet);

        Ok(())
    }

    pub fn get_auto_invest_rule(env: Env, wallet: Address) -> Result<AutoInvestRule, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Rule(wallet))
            .ok_or(Error::RuleNotFound)
    }

    pub fn register_listing(
        env: Env,
        invoice_id: BytesN<32>,
        yield_bps: u32,
        duration_days: u32,
        risk_tier: u32,
        token: Address,
        funding_needed: i128,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if funding_needed <= 0 {
            return Err(Error::InvalidAmount);
        }

        let listing = InvoiceListing {
            invoice_id: invoice_id.clone(),
            yield_bps,
            duration_days,
            risk_tier,
            token,
            funding_needed,
            is_open: true,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Listing(invoice_id), &listing);
        Ok(())
    }

    pub fn execute_auto_invest(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
    ) -> Result<i128, Error> {
        let operator: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Operator)
            .ok_or(Error::NotInitialized)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;

        if caller != operator && caller != admin {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        let mut rule: AutoInvestRule = env
            .storage()
            .persistent()
            .get(&StorageKey::Rule(wallet.clone()))
            .ok_or(Error::RuleNotFound)?;

        if !rule.active {
            return Err(Error::RuleDisabled);
        }

        let mut listing: InvoiceListing = env
            .storage()
            .persistent()
            .get(&StorageKey::Listing(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        if !listing.is_open || listing.funding_needed <= 0 {
            return Err(Error::InvoiceNotOpen);
        }

        // Validate criteria match
        if listing.yield_bps < rule.criteria.min_yield_bps
            || listing.duration_days > rule.criteria.max_duration_days
            || listing.risk_tier > rule.criteria.max_risk_tier
            || listing.token != rule.criteria.token
        {
            return Err(Error::CriteriaMismatch);
        }

        // Check remaining budget
        let remaining_budget = rule
            .total_budget
            .checked_sub(rule.total_invested)
            .ok_or(Error::BudgetExceeded)?;

        if remaining_budget <= 0 {
            return Err(Error::BudgetExceeded);
        }

        // Calculate investment amount
        let mut invest_amount = rule.max_amount_per_invoice;
        if invest_amount > remaining_budget {
            invest_amount = remaining_budget;
        }
        if invest_amount > listing.funding_needed {
            invest_amount = listing.funding_needed;
        }

        if invest_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Transfer first so a short wallet balance cannot leave rule/listing
        // accounting half-applied. Investor auth is required for the pull
        // (operator may invoke, but the wallet must authorize the spend).
        wallet.require_auth();
        let token_client = token::Client::new(&env, &listing.token);
        let wallet_bal = token_client.balance(&wallet);
        if wallet_bal < invest_amount {
            return Err(Error::InvalidAmount);
        }
        token_client.transfer(&wallet, &env.current_contract_address(), &invest_amount);

        rule.total_invested = rule
            .total_invested
            .checked_add(invest_amount)
            .ok_or(Error::BudgetExceeded)?;
        listing.funding_needed = listing
            .funding_needed
            .checked_sub(invest_amount)
            .ok_or(Error::InvalidAmount)?;

        if listing.funding_needed == 0 {
            listing.is_open = false;
        }

        env.storage()
            .persistent()
            .set(&StorageKey::Rule(wallet.clone()), &rule);
        env.storage()
            .persistent()
            .set(&StorageKey::Listing(invoice_id.clone()), &listing);

        events::auto_invest_executed(&env, &wallet, &invoice_id, invest_amount);

        Ok(invest_amount)
    }

    pub fn get_listing(env: Env, invoice_id: BytesN<32>) -> Result<InvoiceListing, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Listing(invoice_id))
            .ok_or(Error::InvoiceNotFound)
    }
}
