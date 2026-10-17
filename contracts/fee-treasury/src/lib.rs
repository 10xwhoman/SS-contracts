#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env};
pub use types::{SplitAmounts, SplitRatios, StorageKey};

fn default_ratios() -> SplitRatios {
    SplitRatios {
        protocol_bps: 5000,
        issuer_bps: 3000,
        lp_pool_bps: 2000,
    }
}

#[contract]
pub struct FeeTreasury;

#[contractimpl]
impl FeeTreasury {
    pub fn initialize(
        env: Env,
        admin: Address,
        market_operator: Address,
        token: Address,
        protocol_recipient: Address,
        lp_pool_recipient: Address,
        ratios: Option<SplitRatios>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        let split = ratios.unwrap_or_else(default_ratios);
        if split.protocol_bps + split.issuer_bps + split.lp_pool_bps != 10_000 {
            return Err(Error::InvalidSplitRatios);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::MarketOperator, &market_operator);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage()
            .instance()
            .set(&StorageKey::ProtocolRecipient, &protocol_recipient);
        env.storage()
            .instance()
            .set(&StorageKey::LpPoolRecipient, &lp_pool_recipient);
        env.storage()
            .instance()
            .set(&StorageKey::SplitRatios, &split);

        Ok(())
    }

    pub fn set_market_operator(env: Env, new_operator: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&StorageKey::MarketOperator, &new_operator);
        Ok(())
    }

    pub fn register_invoice_issuer(
        env: Env,
        invoice_id: BytesN<32>,
        issuer: Address,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        let operator: Address = env
            .storage()
            .instance()
            .get(&StorageKey::MarketOperator)
            .ok_or(Error::NotInitialized)?;

        let caller = if env.storage().instance().has(&StorageKey::Admin) {
            admin.clone()
        } else {
            operator.clone()
        };
        caller.require_auth();

        env.storage()
            .persistent()
            .set(&StorageKey::InvoiceIssuer(invoice_id), &issuer);
        Ok(())
    }

    pub fn update_split_ratios(env: Env, ratios: SplitRatios) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if ratios.protocol_bps + ratios.issuer_bps + ratios.lp_pool_bps != 10_000 {
            return Err(Error::InvalidSplitRatios);
        }

        env.storage()
            .instance()
            .set(&StorageKey::SplitRatios, &ratios);
        events::split_ratios_updated(
            &env,
            ratios.protocol_bps,
            ratios.issuer_bps,
            ratios.lp_pool_bps,
        );

        Ok(())
    }

    pub fn get_split_ratios(env: Env) -> Result<SplitRatios, Error> {
        env.storage()
            .instance()
            .get(&StorageKey::SplitRatios)
            .ok_or(Error::NotInitialized)
    }

    pub fn collect_fee(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        fee_amount: i128,
    ) -> Result<SplitAmounts, Error> {
        let operator: Address = env
            .storage()
            .instance()
            .get(&StorageKey::MarketOperator)
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

        if fee_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let ratios: SplitRatios = env
            .storage()
            .instance()
            .get(&StorageKey::SplitRatios)
            .ok_or(Error::NotInitialized)?;

        let protocol_recipient: Address = env
            .storage()
            .instance()
            .get(&StorageKey::ProtocolRecipient)
            .ok_or(Error::NotInitialized)?;
        let lp_pool_recipient: Address = env
            .storage()
            .instance()
            .get(&StorageKey::LpPoolRecipient)
            .ok_or(Error::NotInitialized)?;

        let issuer: Address = env
            .storage()
            .persistent()
            .get(&StorageKey::InvoiceIssuer(invoice_id.clone()))
            .ok_or(Error::IssuerNotFound)?;

        let protocol_amount = (fee_amount * (ratios.protocol_bps as i128)) / 10_000;
        let issuer_amount = (fee_amount * (ratios.issuer_bps as i128)) / 10_000;
        let lp_pool_amount = fee_amount - protocol_amount - issuer_amount;

        let splits = SplitAmounts {
            protocol_amount,
            issuer_amount,
            lp_pool_amount,
        };

        // Update accumulated balances
        Self::add_balance(&env, &protocol_recipient, protocol_amount);
        Self::add_balance(&env, &issuer, issuer_amount);
        Self::add_balance(&env, &lp_pool_recipient, lp_pool_amount);

        events::fee_collected(&env, &invoice_id, fee_amount, &splits);

        Ok(splits)
    }

    fn add_balance(env: &Env, recipient: &Address, amount: i128) {
        let current = env
            .storage()
            .persistent()
            .get::<_, i128>(&StorageKey::Balance(recipient.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&StorageKey::Balance(recipient.clone()), &(current + amount));

        let total = env
            .storage()
            .persistent()
            .get::<_, i128>(&StorageKey::TotalAccumulated(recipient.clone()))
            .unwrap_or(0);
        env.storage().persistent().set(
            &StorageKey::TotalAccumulated(recipient.clone()),
            &(total + amount),
        );
    }

    pub fn withdraw(env: Env, recipient: Address) -> Result<i128, Error> {
        recipient.require_auth();

        let balance = env
            .storage()
            .persistent()
            .get::<_, i128>(&StorageKey::Balance(recipient.clone()))
            .unwrap_or(0);

        if balance <= 0 {
            return Err(Error::NoBalanceToWithdraw);
        }

        env.storage()
            .persistent()
            .set(&StorageKey::Balance(recipient.clone()), &0i128);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let client = token::Client::new(&env, &token_addr);
        let contract_balance = client.balance(&env.current_contract_address());
        if contract_balance >= balance {
            client.transfer(&env.current_contract_address(), &recipient, &balance);
        }

        events::fee_withdrawn(&env, &recipient, balance);

        Ok(balance)
    }

    pub fn get_balance(env: Env, recipient: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&StorageKey::Balance(recipient))
            .unwrap_or(0)
    }

    pub fn get_total_accumulated(env: Env, recipient: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&StorageKey::TotalAccumulated(recipient))
            .unwrap_or(0)
    }
}
