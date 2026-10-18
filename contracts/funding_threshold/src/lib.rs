#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, Vec, Symbol};

/// Issue #579: Minimum funding threshold with refund on failure
/// 
/// If an invoice does not reach its minimum funding amount before the window closes,
/// all investments are automatically refunded.

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FundingError {
    WindowStillOpen = 1,
    AlreadyChecked = 2,
    InvoiceNotFound = 3,
}

#[contracttype]
#[derive(Clone, PartialEq)]
pub enum InvoiceState {
    Open,
    Live,
    FundingFailed,
    Closed,
}

#[contracttype]
#[derive(Clone)]
pub struct Invoice {
    pub invoice_id: u64,
    pub target_amount: i128,
    pub min_funding_amount: i128,
    pub current_funding: i128,
    pub window_close: u64,
    pub state: InvoiceState,
}

#[contracttype]
#[derive(Clone)]
pub struct Investment {
    pub investor: Address,
    pub amount: i128,
}

#[contracttype]
pub enum DataKey {
    Invoice(u64),
    Investments(u64), // invoice_id -> Vec<Investment>
}

#[contract]
pub struct FundingThresholdContract;

#[contractimpl]
impl FundingThresholdContract {
    /// Create an invoice with minimum funding threshold
    pub fn create_invoice(
        env: Env,
        invoice_id: u64,
        target_amount: i128,
        min_funding_amount: i128,
        window_duration: u64,
    ) {
        let window_close = env.ledger().timestamp() + window_duration;

        let invoice = Invoice {
            invoice_id,
            target_amount,
            min_funding_amount,
            current_funding: 0,
            window_close,
            state: InvoiceState::Open,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id), &invoice);

        // Initialize investments list
        let investments = Vec::<Investment>::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::Investments(invoice_id), &investments);
    }

    /// Record an investment
    pub fn invest(env: Env, invoice_id: u64, investor: Address, amount: i128) {
        investor.require_auth();

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id))
            .expect("Invoice not found");

        if invoice.state != InvoiceState::Open {
            panic!("Invoice not open for funding");
        }

        if env.ledger().timestamp() >= invoice.window_close {
            panic!("Funding window closed");
        }

        invoice.current_funding += amount;

        let mut investments: Vec<Investment> = env
            .storage()
            .persistent()
            .get(&DataKey::Investments(invoice_id))
            .unwrap();

        investments.push_back(Investment {
            investor: investor.clone(),
            amount,
        });

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id), &invoice);
        env.storage()
            .persistent()
            .set(&DataKey::Investments(invoice_id), &investments);
    }

    /// Check funding threshold after window closes
    pub fn check_funding_threshold(env: Env, invoice_id: u64) -> Result<(), FundingError> {
        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id))
            .ok_or(FundingError::InvoiceNotFound)?;

        if env.ledger().timestamp() < invoice.window_close {
            return Err(FundingError::WindowStillOpen);
        }

        if invoice.state != InvoiceState::Open {
            return Err(FundingError::AlreadyChecked);
        }

        if invoice.current_funding < invoice.min_funding_amount {
            // Funding failed - trigger refunds
            invoice.state = InvoiceState::FundingFailed;
            
            // Get all investments
            let investments: Vec<Investment> = env
                .storage()
                .persistent()
                .get(&DataKey::Investments(invoice_id))
                .unwrap();

            // Refund all investors (simulated - would use token transfers in production)
            for investment in investments.iter() {
                env.events().publish(
                    (Symbol::new(&env, "Refund"), invoice_id),
                    (investment.investor.clone(), investment.amount),
                );
            }

            // Emit FundingThresholdMissed event
            env.events().publish(
                (Symbol::new(&env, "FundingThresholdMissed"), invoice_id),
                (
                    invoice.target_amount,
                    invoice.min_funding_amount,
                    invoice.current_funding,
                ),
            );
        } else {
            // Funding successful - proceed to Live
            invoice.state = InvoiceState::Live;

            env.events().publish(
                (Symbol::new(&env, "FundingSuccess"), invoice_id),
                invoice.current_funding,
            );
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id), &invoice);

        Ok(())
    }

    /// Get invoice details
    pub fn get_invoice(env: Env, invoice_id: u64) -> Option<Invoice> {
        env.storage().persistent().get(&DataKey::Invoice(invoice_id))
    }

    /// Get all investments for an invoice
    pub fn get_investments(env: Env, invoice_id: u64) -> Vec<Investment> {
        env.storage()
            .persistent()
            .get(&DataKey::Investments(invoice_id))
            .unwrap_or_else(|| Vec::new(&env))
    }
}
