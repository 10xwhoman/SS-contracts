//! Invoice Default Resolution Contract (#577)
//!
//! Handles defaulted invoices, computes and distributes recovery amounts
//! to fraction holders pro-rata from forfeited collateral.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Vec};

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum InvoiceStatus {
    Active,
    Defaulted,
    RecoveryDistributed,
}

#[derive(Clone)]
#[contracttype]
pub struct DefaultedInvoice {
    pub invoice_id: u64,
    pub status: InvoiceStatus,
    pub collateral_amount: i128,
    pub recovery_amount: i128,
    pub distributed: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct FractionHolder {
    pub address: Address,
    pub fraction_amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Invoice(u64),
    Holders(u64),
}

#[contract]
pub struct DefaultResolutionContract;

#[contractimpl]
impl DefaultResolutionContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Declare an invoice as defaulted (admin-only)
    pub fn declare_default(
        env: Env,
        invoice_id: u64,
        collateral_amount: i128,
        holders: Vec<FractionHolder>,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let invoice_key = DataKey::Invoice(invoice_id);
        
        // Check if invoice already defaulted
        if env.storage().persistent().has(&invoice_key) {
            panic!("Invoice already defaulted");
        }

        // Create defaulted invoice record
        let defaulted_invoice = DefaultedInvoice {
            invoice_id,
            status: InvoiceStatus::Defaulted,
            collateral_amount,
            recovery_amount: 0,
            distributed: false,
        };

        env.storage().persistent().set(&invoice_key, &defaulted_invoice);
        env.storage()
            .persistent()
            .set(&DataKey::Holders(invoice_id), &holders);

        // Emit event
        env.events().publish(
            (symbol_short!("defaulted"), invoice_id),
            collateral_amount,
        );
    }

    /// Distribute recovery amount pro-rata to fraction holders
    pub fn distribute_recovery(env: Env, invoice_id: u64, recovery_amount: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let invoice_key = DataKey::Invoice(invoice_id);
        
        let mut invoice: DefaultedInvoice = env
            .storage()
            .persistent()
            .get(&invoice_key)
            .expect("Invoice not found");

        if invoice.distributed {
            panic!("Recovery already distributed");
        }

        if invoice.status != InvoiceStatus::Defaulted {
            panic!("Invoice not defaulted");
        }

        // Get fraction holders
        let holders: Vec<FractionHolder> = env
            .storage()
            .persistent()
            .get(&DataKey::Holders(invoice_id))
            .expect("Holders not found");

        // Calculate total fractions
        let mut total_fractions = 0i128;
        for holder in holders.iter() {
            total_fractions += holder.fraction_amount;
        }

        if total_fractions == 0 {
            panic!("No fraction holders");
        }

        // Distribute pro-rata
        for holder in holders.iter() {
            let holder_share = (recovery_amount * holder.fraction_amount) / total_fractions;
            
            // Emit distribution event for each holder
            env.events().publish(
                (symbol_short!("distr"), invoice_id, holder.address.clone()),
                holder_share,
            );
        }

        // Update invoice status
        invoice.recovery_amount = recovery_amount;
        invoice.distributed = true;
        invoice.status = InvoiceStatus::RecoveryDistributed;

        env.storage().persistent().set(&invoice_key, &invoice);

        // Emit completion event
        env.events().publish(
            (symbol_short!("recovered"), invoice_id),
            recovery_amount,
        );
    }

    /// Get recovery status for an invoice
    pub fn get_recovery_status(env: Env, invoice_id: u64) -> DefaultedInvoice {
        let invoice_key = DataKey::Invoice(invoice_id);
        
        env.storage()
            .persistent()
            .get(&invoice_key)
            .expect("Invoice not found")
    }

    /// Check if invoice is defaulted
    pub fn is_defaulted(env: Env, invoice_id: u64) -> bool {
        let invoice_key = DataKey::Invoice(invoice_id);
        
        if let Some(invoice) = env
            .storage()
            .persistent()
            .get::<DataKey, DefaultedInvoice>(&invoice_key)
        {
            invoice.status == InvoiceStatus::Defaulted
                || invoice.status == InvoiceStatus::RecoveryDistributed
        } else {
            false
        }
    }

    /// Get holders for an invoice
    pub fn get_holders(env: Env, invoice_id: u64) -> Vec<FractionHolder> {
        env.storage()
            .persistent()
            .get(&DataKey::Holders(invoice_id))
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{vec, Env};

    #[test]
    fn test_declare_default() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DefaultResolutionContract);
        let client = DefaultResolutionContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder1 = Address::generate(&env);
        let holder2 = Address::generate(&env);
        
        client.initialize(&admin);

        let holders = vec![
            &env,
            FractionHolder {
                address: holder1.clone(),
                fraction_amount: 1000,
            },
            FractionHolder {
                address: holder2.clone(),
                fraction_amount: 500,
            },
        ];

        client.declare_default(&1, &100000, &holders);

        let status = client.get_recovery_status(&1);
        assert_eq!(status.status, InvoiceStatus::Defaulted);
        assert!(!status.distributed);
    }

    #[test]
    fn test_distribute_recovery() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DefaultResolutionContract);
        let client = DefaultResolutionContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder1 = Address::generate(&env);
        let holder2 = Address::generate(&env);
        
        client.initialize(&admin);

        let holders = vec![
            &env,
            FractionHolder {
                address: holder1.clone(),
                fraction_amount: 1000,
            },
            FractionHolder {
                address: holder2.clone(),
                fraction_amount: 500,
            },
        ];

        client.declare_default(&1, &100000, &holders);
        
        // Distribute 75000 recovery (75% of collateral)
        client.distribute_recovery(&1, &75000);

        let status = client.get_recovery_status(&1);
        assert_eq!(status.status, InvoiceStatus::RecoveryDistributed);
        assert!(status.distributed);
        assert_eq!(status.recovery_amount, 75000);
    }

    #[test]
    #[should_panic(expected = "Recovery already distributed")]
    fn test_cannot_distribute_twice() {
        let env = Env::default();
        let contract_id = env.register_contract(None, DefaultResolutionContract);
        let client = DefaultResolutionContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder1 = Address::generate(&env);
        
        client.initialize(&admin);

        let holders = vec![
            &env,
            FractionHolder {
                address: holder1.clone(),
                fraction_amount: 1000,
            },
        ];

        client.declare_default(&1, &100000, &holders);
        client.distribute_recovery(&1, &75000);
        
        // Should panic
        client.distribute_recovery(&1, &80000);
    }
}
