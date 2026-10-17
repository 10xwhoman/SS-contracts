#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{InvestorShare, InvoiceInfo, PerInvoiceSummary, StorageKey};

const MAX_BATCH_SIZE: u32 = 20;

#[contract]
pub struct BatchSettlement;

#[contractimpl]
impl BatchSettlement {
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

    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        total_repayment: i128,
        maturity_timestamp: u64,
        is_funded: bool,
        token: Address,
        investors: Vec<InvestorShare>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if total_repayment <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut total_bps: u32 = 0;
        for i in 0..investors.len() {
            let inv = investors.get(i).unwrap();
            total_bps = total_bps
                .checked_add(inv.share_bps)
                .ok_or(Error::InvalidShares)?;
        }
        if total_bps > 10_000 {
            return Err(Error::InvalidShares);
        }

        let info = InvoiceInfo {
            invoice_id: invoice_id.clone(),
            total_repayment,
            maturity_timestamp,
            is_funded,
            is_settled: false,
            token,
            investors,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id), &info);
        Ok(())
    }

    pub fn batch_settle(
        env: Env,
        caller: Address,
        invoice_ids: Vec<BytesN<32>>,
    ) -> Result<Vec<PerInvoiceSummary>, Error> {
        caller.require_auth();

        if invoice_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }

        if invoice_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let current_time = env.ledger().timestamp();

        // 1. Pre-validation pass: If ANY invoice fails validation, the entire batch rejects
        for i in 0..invoice_ids.len() {
            let id = invoice_ids.get(i).unwrap();
            let invoice: InvoiceInfo = env
                .storage()
                .persistent()
                .get(&StorageKey::Invoice(id))
                .ok_or(Error::InvoiceNotFound)?;

            if !invoice.is_funded {
                return Err(Error::InvoiceNotFunded);
            }

            if invoice.is_settled {
                return Err(Error::InvoiceAlreadySettled);
            }

            if current_time < invoice.maturity_timestamp {
                return Err(Error::InvoiceNotMatured);
            }
        }

        // 2. Execution pass: Process pro-rata distributions and state updates atomically
        let mut summaries = Vec::new(&env);

        for i in 0..invoice_ids.len() {
            let id = invoice_ids.get(i).unwrap();
            let mut invoice: InvoiceInfo = env
                .storage()
                .persistent()
                .get(&StorageKey::Invoice(id.clone()))
                .unwrap();

            let mut distributed_amount: i128 = 0;
            let client = token::Client::new(&env, &invoice.token);
            let contract_balance = client.balance(&env.current_contract_address());

            for j in 0..invoice.investors.len() {
                let share = invoice.investors.get(j).unwrap();
                let payout = (invoice.total_repayment * (share.share_bps as i128)) / 10_000;
                distributed_amount = distributed_amount.saturating_add(payout);

                if contract_balance >= payout && payout > 0 {
                    client.transfer(&env.current_contract_address(), &share.investor, &payout);
                }
            }

            invoice.is_settled = true;
            env.storage()
                .persistent()
                .set(&StorageKey::Invoice(id.clone()), &invoice);

            let summary = PerInvoiceSummary {
                invoice_id: id.clone(),
                total_distributed: distributed_amount,
                investor_count: invoice.investors.len(),
                settled_at: current_time,
            };

            env.storage()
                .persistent()
                .set(&StorageKey::SettlementSummary(id), &summary);

            summaries.push_back(summary);
        }

        events::batch_settlement_executed(&env, &caller, summaries.len(), &summaries);

        Ok(summaries)
    }

    pub fn get_invoice_info(env: Env, invoice_id: BytesN<32>) -> Result<InvoiceInfo, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id))
            .ok_or(Error::InvoiceNotFound)
    }

    pub fn get_settlement_summary(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<PerInvoiceSummary, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::SettlementSummary(invoice_id))
            .ok_or(Error::InvoiceNotFound)
    }
}
