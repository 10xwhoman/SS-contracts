#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{HolderShare, InterimPayment, InvoiceSettlementState, StorageKey};

#[contract]
pub struct PartialSettlement;

#[contractimpl]
impl PartialSettlement {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        issuer: Address,
        token: Address,
        total_amount: i128,
        holders: Vec<HolderShare>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut total_bps: u32 = 0;
        for i in 0..holders.len() {
            let h = holders.get(i).unwrap();
            total_bps = total_bps
                .checked_add(h.share_bps)
                .ok_or(Error::InvalidShares)?;
        }
        if total_bps > 10_000 {
            return Err(Error::InvalidShares);
        }

        let state = InvoiceSettlementState {
            invoice_id: invoice_id.clone(),
            issuer,
            token,
            total_amount,
            cumulative_paid: 0,
            remaining_balance: total_amount,
            is_settled: false,
            holders,
            payments: Vec::new(&env),
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id), &state);

        Ok(())
    }

    pub fn partial_settle(env: Env, invoice_id: BytesN<32>, amount: i128) -> Result<i128, Error> {
        let mut state: InvoiceSettlementState = env
            .storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        state.issuer.require_auth();

        if state.is_settled {
            return Err(Error::InvoiceAlreadySettled);
        }

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if amount > state.remaining_balance {
            return Err(Error::AmountExceedsRemaining);
        }

        let current_time = env.ledger().timestamp();
        let client = token::Client::new(&env, &state.token);
        let contract_balance = client.balance(&env.current_contract_address());

        // Pro-rata distribution to holders
        for i in 0..state.holders.len() {
            let share = state.holders.get(i).unwrap();
            let payout = (amount * (share.share_bps as i128)) / 10_000;
            if contract_balance >= payout && payout > 0 {
                client.transfer(&env.current_contract_address(), &share.holder, &payout);
            }
        }

        state.cumulative_paid = state
            .cumulative_paid
            .checked_add(amount)
            .ok_or(Error::InvalidAmount)?;
        state.remaining_balance = state
            .remaining_balance
            .checked_sub(amount)
            .ok_or(Error::InvalidAmount)?;

        if state.remaining_balance == 0 {
            state.is_settled = true;
        }

        let payment_index = state.payments.len() + 1;
        let interim_payment = InterimPayment {
            payment_index,
            amount,
            timestamp: current_time,
            remaining_balance: state.remaining_balance,
        };
        state.payments.push_back(interim_payment);

        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id.clone()), &state);

        events::partial_settlement_executed(
            &env,
            &invoice_id,
            amount,
            state.cumulative_paid,
            state.remaining_balance,
            state.is_settled,
        );

        Ok(state.remaining_balance)
    }

    pub fn get_invoice_state(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<InvoiceSettlementState, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id))
            .ok_or(Error::InvoiceNotFound)
    }

    pub fn get_interim_payments(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Vec<InterimPayment>, Error> {
        let state: InvoiceSettlementState = env
            .storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id))
            .ok_or(Error::InvoiceNotFound)?;
        Ok(state.payments)
    }
}
