//! Accreditation Expiry Enforcement Contract (#575)
//!
//! Tracks wallet accreditation expiry dates, blocks investment on expired
//! accreditation, and emits renewal reminders 30 days before expiry.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

const SECONDS_PER_DAY: u64 = 86400;
const EXPIRY_WARNING_DAYS: u64 = 30;

#[derive(Clone)]
#[contracttype]
pub struct Accreditation {
    pub wallet: Address,
    pub expiry_timestamp: u64,
    pub valid: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Accreditation(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct AccreditationStatus {
    pub valid: bool,
    pub expiry_timestamp: u64,
    pub days_remaining: i64,
}

#[contract]
pub struct AccreditationExpiryContract;

#[contractimpl]
impl AccreditationExpiryContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Set or update accreditation for a wallet (admin-only)
    pub fn set_accreditation(env: Env, wallet: Address, expiry_timestamp: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let current_time = env.ledger().timestamp();
        
        // Emit expiring soon event if expiry is within 30 days
        let days_until_expiry = (expiry_timestamp as i64 - current_time as i64) / SECONDS_PER_DAY as i64;
        if days_until_expiry > 0 && days_until_expiry <= EXPIRY_WARNING_DAYS as i64 {
            env.events().publish(
                (symbol_short!("expiring"), wallet.clone()),
                days_until_expiry,
            );
        }

        let accreditation = Accreditation {
            wallet: wallet.clone(),
            expiry_timestamp,
            valid: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Accreditation(wallet.clone()), &accreditation);

        env.events().publish(
            (symbol_short!("set_accr"), wallet),
            expiry_timestamp,
        );
    }

    /// Check accreditation status for a wallet
    pub fn check_accreditation(env: Env, wallet: Address) -> AccreditationStatus {
        let accreditation_key = DataKey::Accreditation(wallet);
        
        if !env.storage().persistent().has(&accreditation_key) {
            return AccreditationStatus {
                valid: false,
                expiry_timestamp: 0,
                days_remaining: 0,
            };
        }

        let accreditation: Accreditation = env
            .storage()
            .persistent()
            .get(&accreditation_key)
            .unwrap();

        let current_time = env.ledger().timestamp();
        let is_valid = accreditation.valid && current_time < accreditation.expiry_timestamp;
        
        let days_remaining = if accreditation.expiry_timestamp > current_time {
            ((accreditation.expiry_timestamp - current_time) / SECONDS_PER_DAY) as i64
        } else {
            0
        };

        AccreditationStatus {
            valid: is_valid,
            expiry_timestamp: accreditation.expiry_timestamp,
            days_remaining,
        }
    }

    /// Revoke accreditation immediately (admin-only)
    pub fn revoke_accreditation(env: Env, wallet: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let accreditation_key = DataKey::Accreditation(wallet.clone());
        
        if let Some(mut accreditation) = env
            .storage()
            .persistent()
            .get::<DataKey, Accreditation>(&accreditation_key)
        {
            accreditation.valid = false;
            env.storage()
                .persistent()
                .set(&accreditation_key, &accreditation);

            env.events().publish(
                (symbol_short!("revoked"), wallet),
                (),
            );
        }
    }

    /// Validate investment (called by investment contract)
    /// Returns true if accreditation is valid, panics with AccreditationExpired if not
    pub fn validate_investment(env: Env, wallet: Address) -> bool {
        let status = Self::check_accreditation(env.clone(), wallet.clone());
        
        if !status.valid {
            panic!("AccreditationExpired");
        }

        // Emit warning if expiring soon
        if status.days_remaining <= EXPIRY_WARNING_DAYS as i64 && status.days_remaining > 0 {
            env.events().publish(
                (symbol_short!("expiring"), wallet),
                status.days_remaining,
            );
        }

        true
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    #[test]
    fn test_set_and_check_accreditation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AccreditationExpiryContract);
        let client = AccreditationExpiryContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);

        // Set accreditation 60 days in future
        let expiry = env.ledger().timestamp() + (60 * SECONDS_PER_DAY);
        client.set_accreditation(&wallet, &expiry);

        let status = client.check_accreditation(&wallet);
        assert!(status.valid);
        assert_eq!(status.expiry_timestamp, expiry);
        assert_eq!(status.days_remaining, 60);
    }

    #[test]
    fn test_revoke_accreditation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AccreditationExpiryContract);
        let client = AccreditationExpiryContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);

        let expiry = env.ledger().timestamp() + (60 * SECONDS_PER_DAY);
        client.set_accreditation(&wallet, &expiry);

        // Revoke
        client.revoke_accreditation(&wallet);

        let status = client.check_accreditation(&wallet);
        assert!(!status.valid);
    }

    #[test]
    #[should_panic(expected = "AccreditationExpired")]
    fn test_validate_investment_expired() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AccreditationExpiryContract);
        let client = AccreditationExpiryContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);

        // Set expiry in the past
        let expiry = env.ledger().timestamp() - SECONDS_PER_DAY;
        client.set_accreditation(&wallet, &expiry);

        // Should panic
        client.validate_investment(&wallet);
    }
}
