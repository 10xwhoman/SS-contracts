//! Protocol Insurance Pool implementation for bad-debt coverage (#479).
//!
//! Provides automated insurance deductions during invoice registration to build
//! a reserve pool that protects investors against defaulted or uncollectible invoices.

use soroban_sdk::{contracttype, Address, BytesN, Env, Symbol};

/// Storage key for the protocol insurance pool configuration and balance.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InsuranceStorageKey {
    /// Instance: insurance pool configuration (admin, fee_bps, recipient).
    Config,
    /// Instance: total accumulated balance in the insurance pool (stroops).
    PoolBalance,
    /// Persistent: record of insurance claims paid out per invoice.
    ClaimRecord(BytesN<32>),
}

/// Configuration for the protocol insurance pool.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceConfig {
    /// Admin authorized to update pool parameters and approve bad-debt claims.
    pub admin: Address,
    /// Deduction rate in basis points (e.g. 50 = 0.5%, 100 = 1.0%). Max 1000 bps (10%).
    pub deduction_bps: u32,
    /// Address holding or receiving designated insurance pool reserves.
    pub pool_address: Address,
    /// Whether automated deduction is active on invoice registration.
    pub is_active: bool,
}

/// Event emitted when an insurance deduction is made on invoice registration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceDeductedEvent {
    pub invoice_id: BytesN<32>,
    pub funding_target: i128,
    pub deducted_amount: i128,
    pub new_pool_balance: i128,
}

/// Event emitted when bad-debt coverage is claimed from the insurance pool.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaimedEvent {
    pub invoice_id: BytesN<32>,
    pub recipient: Address,
    pub claim_amount: i128,
    pub remaining_pool_balance: i128,
}

pub struct InsurancePool;

impl InsurancePool {
    /// Maximum allowed insurance deduction rate (1000 bps = 10%).
    pub const MAX_DEDUCTION_BPS: u32 = 1_000;

    /// Initialize or update the insurance pool configuration.
    pub fn init_config(env: &Env, config: &InsuranceConfig) {
        if config.deduction_bps > Self::MAX_DEDUCTION_BPS {
            panic!("Insurance deduction exceeds maximum allowed rate");
        }
        env.storage().instance().set(&InsuranceStorageKey::Config, config);
    }

    /// Retrieve the current insurance pool configuration.
    pub fn get_config(env: &Env) -> Option<InsuranceConfig> {
        env.storage().instance().get(&InsuranceStorageKey::Config)
    }

    /// Retrieve the current total insurance pool reserve balance.
    pub fn get_balance(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&InsuranceStorageKey::PoolBalance)
            .unwrap_or(0)
    }

    /// Calculates and records the insurance pool deduction on invoice registration.
    /// Returns the deducted amount.
    pub fn record_invoice_deduction(env: &Env, invoice_id: &BytesN<32>, funding_target: i128) -> i128 {
        let config = match Self::get_config(env) {
            Some(cfg) if cfg.is_active && cfg.deduction_bps > 0 => cfg,
            _ => return 0,
        };

        if funding_target <= 0 {
            return 0;
        }

        // Calculate deduction: (funding_target * deduction_bps) / 10000
        let deduction = (funding_target * config.deduction_bps as i128) / 10_000;
        if deduction <= 0 {
            return 0;
        }

        let current_balance = Self::get_balance(env);
        let new_balance = current_balance + deduction;
        env.storage().instance().set(&InsuranceStorageKey::PoolBalance, &new_balance);

        env.events().publish(
            (Symbol::new(env, "insurance"), Symbol::new(env, "deducted")),
            InsuranceDeductedEvent {
                invoice_id: invoice_id.clone(),
                funding_target,
                deducted_amount: deduction,
                new_pool_balance: new_balance,
            },
        );

        deduction
    }

    /// Claims coverage from the insurance pool to compensate investors for bad debt.
    /// Requires admin authorization.
    pub fn claim_coverage(
        env: &Env,
        admin: &Address,
        invoice_id: &BytesN<32>,
        recipient: &Address,
        claim_amount: i128,
    ) -> i128 {
        admin.require_auth();

        let config = Self::get_config(env).expect("Insurance pool not initialized");
        if config.admin != *admin {
            panic!("Unauthorized: only insurance pool admin can approve claims");
        }

        if claim_amount <= 0 {
            panic!("Claim amount must be positive");
        }

        let current_balance = Self::get_balance(env);
        if claim_amount > current_balance {
            panic!("Claim amount exceeds available insurance pool reserves");
        }

        let new_balance = current_balance - claim_amount;
        env.storage().instance().set(&InsuranceStorageKey::PoolBalance, &new_balance);
        env.storage().persistent().set(
            &InsuranceStorageKey::ClaimRecord(invoice_id.clone()),
            &claim_amount,
        );

        env.events().publish(
            (Symbol::new(env, "insurance"), Symbol::new(env, "claimed")),
            InsuranceClaimedEvent {
                invoice_id: invoice_id.clone(),
                recipient: recipient.clone(),
                claim_amount,
                remaining_pool_balance: new_balance,
            },
        );

        claim_amount
    }
}
