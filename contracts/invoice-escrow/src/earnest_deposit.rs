//! Debtor Commitment Earnest Money Deposit implementation (#481).
//!
//! Enforces an earnest money deposit requirement on debtor commitment during
//! invoice registration. The deposit provides protocol security and investor
//! assurance against default.
//!
//! Lifecycle:
//! 1. `lock_deposit`: Required percentage/amount deposited and locked on invoice creation.
//! 2. `return_deposit`: Full deposit returned to debtor upon full invoice settlement.
//! 3. `forfeit_deposit`: Deposit forfeited and routed to investors or insurance pool upon default/refund.

use soroban_sdk::{contracttype, Address, BytesN, Env, Symbol};

/// Error types for earnest money deposit operations.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EarnestError {
    NotConfigured = 1,
    Unauthorized = 2,
    DepositTooLow = 3,
    DepositAlreadyExists = 4,
    DepositNotFound = 5,
    InvalidStatus = 6,
    ZeroAmount = 7,
}

/// Lifecycle status of an earnest deposit.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EarnestStatus {
    /// Deposit is locked in escrow during funding/payment window.
    Locked = 1,
    /// Deposit has been returned to debtor upon full settlement.
    Returned = 2,
    /// Deposit has been forfeited due to invoice default or cancellation.
    Forfeited = 3,
}

/// Configuration parameters for earnest money deposits.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarnestDepositConfig {
    /// Protocol admin authorized to update config parameters.
    pub admin: Address,
    /// Required deposit rate in basis points (e.g. 500 = 5%). Max 2000 bps (20%).
    pub deposit_bps: u32,
    /// Minimum absolute earnest deposit amount (in stroops / base token units).
    pub min_deposit: i128,
    /// Whether earnest deposit is mandatory for invoice creation.
    pub is_required: bool,
}

/// Record of an earnest deposit locked for a specific invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarnestDepositRecord {
    pub invoice_id: BytesN<32>,
    pub debtor: Address,
    pub amount: i128,
    pub status: EarnestStatus,
    pub locked_timestamp: u64,
    pub resolved_timestamp: u64,
}

/// Storage keys for earnest deposit state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EarnestStorageKey {
    /// Instance: global earnest deposit config.
    Config,
    /// Instance: total cumulative earnest deposits currently locked.
    TotalLocked,
    /// Instance: total cumulative earnest deposits forfeited to pool/investors.
    TotalForfeited,
    /// Persistent: earnest deposit record by 32-byte invoice id.
    DepositByBytes(BytesN<32>),
    /// Persistent: earnest deposit record by Symbol invoice id.
    DepositBySymbol(Symbol),
}

/// Event emitted when earnest money is locked for an invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarnestDepositLockedEvent {
    pub invoice_id: BytesN<32>,
    pub debtor: Address,
    pub amount: i128,
    pub total_locked: i128,
}

/// Event emitted when earnest money is returned to debtor upon settlement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarnestDepositReturnedEvent {
    pub invoice_id: BytesN<32>,
    pub debtor: Address,
    pub amount: i128,
    pub total_locked: i128,
}

/// Event emitted when earnest money is forfeited on default/refund.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarnestDepositForfeitedEvent {
    pub invoice_id: BytesN<32>,
    pub debtor: Address,
    pub amount: i128,
    pub beneficiary: Address,
    pub total_forfeited: i128,
}

pub struct EarnestDeposit;

impl EarnestDeposit {
    /// Maximum allowed earnest deposit rate: 2,000 bps (20%).
    pub const MAX_DEPOSIT_BPS: u32 = 2_000;

    /// Initializes or updates earnest deposit configuration.
    pub fn init_config(env: &Env, config: &EarnestDepositConfig) -> Result<(), EarnestError> {
        if config.deposit_bps > Self::MAX_DEPOSIT_BPS {
            panic!("Deposit bps exceeds maximum allowed rate of 20%");
        }
        env.storage().instance().set(&EarnestStorageKey::Config, config);
        Ok(())
    }

    /// Retrieves current configuration.
    pub fn get_config(env: &Env) -> Option<EarnestDepositConfig> {
        env.storage().instance().get(&EarnestStorageKey::Config)
    }

    /// Calculates required earnest deposit for a given face value.
    pub fn calculate_required_deposit(env: &Env, face_value: i128) -> i128 {
        let config = match Self::get_config(env) {
            Some(cfg) if cfg.is_required => cfg,
            _ => return 0,
        };

        if face_value <= 0 {
            return 0;
        }

        let calculated = (face_value * config.deposit_bps as i128) / 10_000;
        if calculated < config.min_deposit {
            config.min_deposit
        } else {
            calculated
        }
    }

    /// Returns the total earnest money currently locked across all active invoices.
    pub fn get_total_locked(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&EarnestStorageKey::TotalLocked)
            .unwrap_or(0)
    }

    /// Returns the total earnest money forfeited.
    pub fn get_total_forfeited(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&EarnestStorageKey::TotalForfeited)
            .unwrap_or(0)
    }

    /// Locks an earnest deposit for an invoice identified by BytesN<32>.
    pub fn lock_deposit(
        env: &Env,
        invoice_id: &BytesN<32>,
        debtor: &Address,
        amount: i128,
    ) -> Result<EarnestDepositRecord, EarnestError> {
        debtor.require_auth();

        if amount <= 0 {
            return Err(EarnestError::ZeroAmount);
        }

        if let Some(config) = Self::get_config(env) {
            if config.is_required && amount < config.min_deposit {
                return Err(EarnestError::DepositTooLow);
            }
        }

        let key = EarnestStorageKey::DepositByBytes(invoice_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(EarnestError::DepositAlreadyExists);
        }

        let current_time = env.ledger().timestamp();
        let record = EarnestDepositRecord {
            invoice_id: invoice_id.clone(),
            debtor: debtor.clone(),
            amount,
            status: EarnestStatus::Locked,
            locked_timestamp: current_time,
            resolved_timestamp: 0,
        };

        env.storage().persistent().set(&key, &record);

        let total_locked = Self::get_total_locked(env) + amount;
        env.storage().instance().set(&EarnestStorageKey::TotalLocked, &total_locked);

        env.events().publish(
            (Symbol::new(env, "earnest"), Symbol::new(env, "locked")),
            EarnestDepositLockedEvent {
                invoice_id: invoice_id.clone(),
                debtor: debtor.clone(),
                amount,
                total_locked,
            },
        );

        Ok(record)
    }

    /// Retrieves an earnest deposit record by BytesN<32>.
    pub fn get_deposit(env: &Env, invoice_id: &BytesN<32>) -> Option<EarnestDepositRecord> {
        env.storage()
            .persistent()
            .get(&EarnestStorageKey::DepositByBytes(invoice_id.clone()))
    }

    /// Returns the earnest deposit to debtor upon full invoice settlement.
    pub fn return_deposit(env: &Env, invoice_id: &BytesN<32>) -> Result<i128, EarnestError> {
        let key = EarnestStorageKey::DepositByBytes(invoice_id.clone());
        let mut record: EarnestDepositRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(EarnestError::DepositNotFound)?;

        if record.status != EarnestStatus::Locked {
            return Err(EarnestError::InvalidStatus);
        }

        let amount = record.amount;
        record.status = EarnestStatus::Returned;
        record.resolved_timestamp = env.ledger().timestamp();
        env.storage().persistent().set(&key, &record);

        let current_locked = Self::get_total_locked(env);
        let new_locked = current_locked.saturating_sub(amount);
        env.storage().instance().set(&EarnestStorageKey::TotalLocked, &new_locked);

        env.events().publish(
            (Symbol::new(env, "earnest"), Symbol::new(env, "returned")),
            EarnestDepositReturnedEvent {
                invoice_id: invoice_id.clone(),
                debtor: record.debtor,
                amount,
                total_locked: new_locked,
            },
        );

        Ok(amount)
    }

    /// Forfeits the earnest deposit upon default or refund and routes it to the designated beneficiary.
    pub fn forfeit_deposit(
        env: &Env,
        invoice_id: &BytesN<32>,
        beneficiary: &Address,
    ) -> Result<i128, EarnestError> {
        let key = EarnestStorageKey::DepositByBytes(invoice_id.clone());
        let mut record: EarnestDepositRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(EarnestError::DepositNotFound)?;

        if record.status != EarnestStatus::Locked {
            return Err(EarnestError::InvalidStatus);
        }

        let amount = record.amount;
        record.status = EarnestStatus::Forfeited;
        record.resolved_timestamp = env.ledger().timestamp();
        env.storage().persistent().set(&key, &record);

        let current_locked = Self::get_total_locked(env);
        let new_locked = current_locked.saturating_sub(amount);
        env.storage().instance().set(&EarnestStorageKey::TotalLocked, &new_locked);

        let current_forfeited = Self::get_total_forfeited(env);
        let new_forfeited = current_forfeited + amount;
        env.storage().instance().set(&EarnestStorageKey::TotalForfeited, &new_forfeited);

        env.events().publish(
            (Symbol::new(env, "earnest"), Symbol::new(env, "forfeited")),
            EarnestDepositForfeitedEvent {
                invoice_id: invoice_id.clone(),
                debtor: record.debtor,
                amount,
                beneficiary: beneficiary.clone(),
                total_forfeited: new_forfeited,
            },
        );

        Ok(amount)
    }
}
