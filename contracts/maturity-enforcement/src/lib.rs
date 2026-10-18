#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol,
};

const SECONDS_PER_DAY: u64 = 86_400;
const GRACE_PERIOD_DAYS: u64 = 7;
const DEFAULT_TRIGGER_DAYS: u64 = 30;
// Late penalty: 0.1% per day = 1 / 1000 = 10 BPS per day
const PENALTY_BPS_PER_DAY: i128 = 10;
const BPS_DENOMINATOR: i128 = 10_000;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Invoice(Symbol),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InvoiceMaturityData {
    pub due_date: u64,
    pub principal_amount: i128,
    pub outstanding_amount: i128,
    pub accrued_penalty: i128,
    pub last_checked_timestamp: u64,
    pub maturity_missed_emitted: bool,
    pub default_triggered: bool,
    pub is_settled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MaturityStatus {
    pub status: Symbol,
    pub due_date: u64,
    pub outstanding_amount: i128,
    pub accrued_penalty: i128,
    pub total_due: i128,
    pub overdue_days: u64,
    pub default_triggered: bool,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvoiceNotFound = 4,
    InvoiceAlreadyExists = 5,
    InvalidAmount = 6,
    InvalidDueDate = 7,
}

#[contract]
pub struct MaturityEnforcement;

#[contractimpl]
impl MaturityEnforcement {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register an invoice with due date and principal amount
    pub fn register_invoice(
        env: Env,
        caller: Address,
        invoice_id: Symbol,
        due_date: u64,
        amount: i128,
    ) -> Result<(), Error> {
        caller.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if env.storage().persistent().has(&DataKey::Invoice(invoice_id.clone())) {
            return Err(Error::InvoiceAlreadyExists);
        }

        let invoice_data = InvoiceMaturityData {
            due_date,
            principal_amount: amount,
            outstanding_amount: amount,
            accrued_penalty: 0,
            last_checked_timestamp: env.ledger().timestamp(),
            maturity_missed_emitted: false,
            default_triggered: false,
            is_settled: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id.clone()), &invoice_data);

        env.events().publish(
            (Symbol::new(&env, "InvoiceRegistered"), invoice_id),
            (due_date, amount),
        );

        Ok(())
    }

    /// Check maturity status of an invoice.
    /// Callable by ANY wallet/caller without authorization.
    /// Enforces 7-day grace period, 0.1% per day late penalty, and default after 30 days overdue.
    pub fn check_maturity(env: Env, invoice_id: Symbol) -> Result<MaturityStatus, Error> {
        let mut data: InvoiceMaturityData = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        let current_timestamp = env.ledger().timestamp();
        data.last_checked_timestamp = current_timestamp;

        if data.is_settled {
            let status = Symbol::new(&env, "settled");
            return Ok(MaturityStatus {
                status,
                due_date: data.due_date,
                outstanding_amount: 0,
                accrued_penalty: data.accrued_penalty,
                total_due: 0,
                overdue_days: 0,
                default_triggered: data.default_triggered,
            });
        }

        // Case 1: Before or on due date -> Active
        if current_timestamp <= data.due_date {
            let status = Symbol::new(&env, "active");
            return Ok(MaturityStatus {
                status,
                due_date: data.due_date,
                outstanding_amount: data.outstanding_amount,
                accrued_penalty: 0,
                total_due: data.outstanding_amount,
                overdue_days: 0,
                default_triggered: false,
            });
        }

        // Case 2: Past due date
        // Emit MaturityMissed if not yet emitted
        if !data.maturity_missed_emitted {
            data.maturity_missed_emitted = true;
            env.events().publish(
                (Symbol::new(&env, "MaturityMissed"), invoice_id.clone()),
                (data.due_date, data.outstanding_amount),
            );
        }

        let elapsed_seconds = current_timestamp.saturating_sub(data.due_date);
        let overdue_days = elapsed_seconds / SECONDS_PER_DAY;

        let mut status = Symbol::new(&env, "grace_period");

        // Grace period check: 7 days after due date before penalty accrues
        if overdue_days > GRACE_PERIOD_DAYS {
            let penalty_days = overdue_days.saturating_sub(GRACE_PERIOD_DAYS);
            // 0.1% per day = 10 BPS per day on outstanding amount
            let penalty = (data.outstanding_amount
                .saturating_mul(PENALTY_BPS_PER_DAY)
                .saturating_mul(penalty_days as i128))
                / BPS_DENOMINATOR;

            if penalty > data.accrued_penalty {
                data.accrued_penalty = penalty;
                env.events().publish(
                    (Symbol::new(&env, "PenaltyAccrued"), invoice_id.clone()),
                    (penalty, penalty_days),
                );
            }
            status = Symbol::new(&env, "overdue");
        }

        // Default trigger check: after 30 days overdue with no payment
        if overdue_days >= DEFAULT_TRIGGER_DAYS && !data.default_triggered {
            data.default_triggered = true;
            let total_due = data.outstanding_amount.saturating_add(data.accrued_penalty);
            status = Symbol::new(&env, "defaulted");
            env.events().publish(
                (Symbol::new(&env, "DefaultTriggered"), invoice_id.clone()),
                (data.outstanding_amount, total_due),
            );
        } else if data.default_triggered {
            status = Symbol::new(&env, "defaulted");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id.clone()), &data);

        let total_due = data.outstanding_amount.saturating_add(data.accrued_penalty);

        Ok(MaturityStatus {
            status,
            due_date: data.due_date,
            outstanding_amount: data.outstanding_amount,
            accrued_penalty: data.accrued_penalty,
            total_due,
            overdue_days,
            default_triggered: data.default_triggered,
        })
    }

    /// Record payment against invoice, reducing outstanding amount
    pub fn record_payment(
        env: Env,
        caller: Address,
        invoice_id: Symbol,
        amount: i128,
    ) -> Result<i128, Error> {
        caller.require_auth();

        let mut data: InvoiceMaturityData = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        data.outstanding_amount = data.outstanding_amount.saturating_sub(amount);
        if data.outstanding_amount == 0 {
            data.is_settled = true;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id.clone()), &data);

        env.events().publish(
            (Symbol::new(&env, "PaymentRecorded"), invoice_id),
            (amount, data.outstanding_amount),
        );

        Ok(data.outstanding_amount)
    }

    pub fn get_invoice(env: Env, invoice_id: Symbol) -> Option<InvoiceMaturityData> {
        env.storage().persistent().get(&DataKey::Invoice(invoice_id))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Env};

    #[test]
    fn test_maturity_enforcement_grace_penalty_and_default() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let any_wallet = Address::generate(&env);
        let invoice_id = Symbol::new(&env, "INV-MAT-100");

        let contract_id = env.register(MaturityEnforcement, ());
        let client = MaturityEnforcementClient::new(&env, &contract_id);

        client.initialize(&admin);

        let due_date: u64 = 1_000_000;
        let principal: i128 = 100_000; // e.g. 100,000 units

        // Register invoice
        client.register_invoice(&admin, &invoice_id, &due_date, &principal);

        // 1. Check maturity BEFORE due date (timestamp = 900_000)
        env.ledger().set_timestamp(900_000);
        let status = client.check_maturity(&invoice_id);
        assert_eq!(status.status, Symbol::new(&env, "active"));
        assert_eq!(status.accrued_penalty, 0);
        assert_eq!(status.overdue_days, 0);
        assert!(!status.default_triggered);

        // 2. Check maturity WITHIN grace period (timestamp = due_date + 4 days)
        env.ledger().set_timestamp(due_date + 4 * SECONDS_PER_DAY);
        // Callable by ANY wallet
        let status_grace = client.check_maturity(&invoice_id);
        assert_eq!(status_grace.status, Symbol::new(&env, "grace_period"));
        assert_eq!(status_grace.accrued_penalty, 0); // No penalty in grace period!
        assert_eq!(status_grace.overdue_days, 4);
        assert!(!status_grace.default_triggered);

        // 3. Check maturity AFTER grace period (timestamp = due_date + 12 days)
        // Overdue days = 12, penalty days = 12 - 7 = 5 days
        // Penalty = 100,000 * 10 * 5 / 10,000 = 500 (0.1% per day * 5 days = 0.5% = 500)
        env.ledger().set_timestamp(due_date + 12 * SECONDS_PER_DAY);
        let status_penalty = client.check_maturity(&invoice_id);
        assert_eq!(status_penalty.status, Symbol::new(&env, "overdue"));
        assert_eq!(status_penalty.accrued_penalty, 500);
        assert_eq!(status_penalty.total_due, 100_500);
        assert_eq!(status_penalty.overdue_days, 12);
        assert!(!status_penalty.default_triggered);

        // 4. Check maturity AFTER 30 days overdue (timestamp = due_date + 31 days)
        // Default triggered!
        // Overdue days = 31, penalty days = 31 - 7 = 24 days
        // Penalty = 100,000 * 10 * 24 / 10,000 = 2400
        env.ledger().set_timestamp(due_date + 31 * SECONDS_PER_DAY);
        let status_default = client.check_maturity(&invoice_id);
        assert_eq!(status_default.status, Symbol::new(&env, "defaulted"));
        assert_eq!(status_default.accrued_penalty, 2400);
        assert_eq!(status_default.total_due, 102_400);
        assert_eq!(status_default.overdue_days, 31);
        assert!(status_default.default_triggered);

        // Record full payment
        client.record_payment(&admin, &invoice_id, &100_000);
        let status_settled = client.check_maturity(&invoice_id);
        assert_eq!(status_settled.status, Symbol::new(&env, "settled"));
    }
}
