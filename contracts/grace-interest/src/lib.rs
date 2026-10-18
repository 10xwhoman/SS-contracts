#![no_std]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Symbol, Vec,
};

const SECONDS_PER_DAY: u64 = 86_400;
/// 0.15% per day on outstanding principal.
const DAILY_RATE_BPS: i128 = 15;
const BPS_DENOMINATOR: i128 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvoiceAlreadyRegistered = 3,
    InvoiceNotFound = 4,
    InvalidAmount = 5,
    GracePeriodNotStarted = 6,
    NothingToAccrue = 7,
    AlreadySettled = 8,
    Overflow = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Holding {
    pub holder: Address,
    pub fractions: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LateInvoice {
    pub token: Address,
    pub principal: i128,
    pub grace_start: u64,
    pub last_accrual: u64,
    pub accrued: i128,
    pub settled: bool,
    pub holdings: Vec<Holding>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Invoice(BytesN<32>),
}

#[contract]
pub struct GraceInterest;

#[contractimpl]
impl GraceInterest {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Registers an overdue invoice. Interest accrues from `grace_start`.
    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        token: Address,
        principal: i128,
        grace_start: u64,
        holdings: Vec<Holding>,
    ) -> Result<(), Error> {
        Self::require_admin(&env)?;
        let key = StorageKey::Invoice(invoice_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::InvoiceAlreadyRegistered);
        }
        let mut total_fractions: i128 = 0;
        for h in holdings.iter() {
            if h.fractions <= 0 {
                return Err(Error::InvalidAmount);
            }
            total_fractions = total_fractions
                .checked_add(h.fractions)
                .ok_or(Error::Overflow)?;
        }
        if principal <= 0 || total_fractions == 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage().persistent().set(
            &key,
            &LateInvoice {
                token,
                principal,
                grace_start,
                last_accrual: grace_start,
                accrued: 0,
                settled: false,
                holdings,
            },
        );
        Ok(())
    }

    /// Accrues 0.15% of principal for every full day elapsed since the last accrual.
    #[allow(deprecated)]
    pub fn accrue_interest(env: Env, invoice_id: BytesN<32>) -> Result<i128, Error> {
        let mut invoice = Self::load(&env, &invoice_id)?;
        if invoice.settled {
            return Err(Error::AlreadySettled);
        }
        let now = env.ledger().timestamp();
        if now < invoice.grace_start {
            return Err(Error::GracePeriodNotStarted);
        }
        let days = (now - invoice.last_accrual) / SECONDS_PER_DAY;
        if days == 0 {
            return Err(Error::NothingToAccrue);
        }
        let amount = invoice
            .principal
            .checked_mul(DAILY_RATE_BPS)
            .and_then(|v| v.checked_mul(days as i128))
            .ok_or(Error::Overflow)?
            / BPS_DENOMINATOR;
        invoice.accrued = invoice.accrued.checked_add(amount).ok_or(Error::Overflow)?;
        invoice.last_accrual += days * SECONDS_PER_DAY;

        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id.clone()), &invoice);
        let topics = (Symbol::new(&env, "InterestAccrued"), invoice_id);
        env.events().publish(topics, (amount, invoice.accrued));
        Ok(amount)
    }

    pub fn get_accrued_interest(env: Env, invoice_id: BytesN<32>) -> Result<i128, Error> {
        Ok(Self::load(&env, &invoice_id)?.accrued)
    }

    /// Pays the accrued interest from `payer` to fraction holders pro-rata and closes accrual.
    pub fn settle_interest(env: Env, invoice_id: BytesN<32>, payer: Address) -> Result<(), Error> {
        payer.require_auth();
        let mut invoice = Self::load(&env, &invoice_id)?;
        if invoice.settled {
            return Err(Error::AlreadySettled);
        }
        let mut total_fractions: i128 = 0;
        for h in invoice.holdings.iter() {
            total_fractions += h.fractions;
        }
        let client = token::Client::new(&env, &invoice.token);
        let mut distributed: i128 = 0;
        let last = invoice.holdings.len() - 1;
        for (i, h) in invoice.holdings.iter().enumerate() {
            // The final holder takes the rounding remainder so the full amount is paid out.
            let share = if i as u32 == last {
                invoice.accrued - distributed
            } else {
                invoice
                    .accrued
                    .checked_mul(h.fractions)
                    .ok_or(Error::Overflow)?
                    / total_fractions
            };
            distributed += share;
            if share > 0 {
                client.transfer(&payer, &h.holder, &share);
            }
        }
        invoice.settled = true;
        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id), &invoice);
        Ok(())
    }
}

impl GraceInterest {
    fn load(env: &Env, invoice_id: &BytesN<32>) -> Result<LateInvoice, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)
    }

    fn require_admin(env: &Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }
}
