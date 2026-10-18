#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Lock(Symbol, Address),
    Rule(Symbol, Symbol),                   // (invoice_id, rule_type) -> bool
    BlockedJurisdiction(Symbol, Symbol),    // (invoice_id, jurisdiction) -> bool
    WalletKyc(Address),                     // wallet -> bool
    WalletJurisdiction(Address),            // wallet -> Symbol
    WalletAccredited(Address),              // wallet -> bool
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Lockup {
    pub unlock_at: u64,
    pub expired_emitted: bool,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidDuration = 3,
    TransferLocked = 4,
    TransferBlocked = 5,
    InvalidRuleType = 6,
    InvalidAmount = 7,
}

#[contract]
pub struct TransferRestriction;

#[contractimpl]
impl TransferRestriction {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Admin-only: configure transfer restriction rules for an invoice
    /// Supported rule types: kyc_required, jurisdiction_block, accreditation_required
    pub fn set_restriction_rule(
        env: Env,
        invoice_id: Symbol,
        rule_type: Symbol,
        enabled: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let kyc = Symbol::new(&env, "kyc_required");
        let jur = Symbol::new(&env, "jurisdiction_block");
        let acc = Symbol::new(&env, "accreditation_required");

        if rule_type != kyc && rule_type != jur && rule_type != acc {
            return Err(Error::InvalidRuleType);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Rule(invoice_id.clone(), rule_type.clone()), &enabled);

        env.events().publish(
            (Symbol::new(&env, "RestrictionRuleSet"), invoice_id, rule_type),
            enabled,
        );

        Ok(())
    }

    /// Admin-only: configure blocked jurisdictions for a specific invoice
    pub fn set_blocked_jurisdiction(
        env: Env,
        invoice_id: Symbol,
        jurisdiction: Symbol,
        blocked: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage().persistent().set(
            &DataKey::BlockedJurisdiction(invoice_id.clone(), jurisdiction.clone()),
            &blocked,
        );

        Ok(())
    }

    /// Admin-only: update KYC status for a wallet
    pub fn set_wallet_kyc(env: Env, wallet: Address, verified: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::WalletKyc(wallet), &verified);

        Ok(())
    }

    /// Admin-only: update jurisdiction for a wallet
    pub fn set_wallet_jurisdiction(
        env: Env,
        wallet: Address,
        jurisdiction: Symbol,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::WalletJurisdiction(wallet), &jurisdiction);

        Ok(())
    }

    /// Admin-only: update accreditation status for a wallet
    pub fn set_wallet_accreditation(
        env: Env,
        wallet: Address,
        accredited: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::WalletAccredited(wallet), &accredited);

        Ok(())
    }

    pub fn get_wallet_kyc(env: Env, wallet: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::WalletKyc(wallet))
            .unwrap_or(false)
    }

    pub fn get_wallet_jurisdiction(env: Env, wallet: Address) -> Option<Symbol> {
        env.storage()
            .persistent()
            .get(&DataKey::WalletJurisdiction(wallet))
    }

    pub fn get_wallet_accreditation(env: Env, wallet: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::WalletAccredited(wallet))
            .unwrap_or(false)
    }

    pub fn is_rule_enabled(env: Env, invoice_id: Symbol, rule_type: Symbol) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Rule(invoice_id, rule_type))
            .unwrap_or(false)
    }

    pub fn is_jurisdiction_blocked(env: Env, invoice_id: Symbol, jurisdiction: Symbol) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::BlockedJurisdiction(invoice_id, jurisdiction))
            .unwrap_or(false)
    }

    /// Checks whether transfer between `from` and `to` is permitted under invoice regulatory rules.
    /// Returns (allowed: bool, reason: Symbol). Emits TransferBlocked if rejected.
    pub fn check_transfer(
        env: Env,
        from: Address,
        to: Address,
        invoice_id: Symbol,
    ) -> (bool, Symbol) {
        let kyc_rule = Symbol::new(&env, "kyc_required");
        let jur_rule = Symbol::new(&env, "jurisdiction_block");
        let acc_rule = Symbol::new(&env, "accreditation_required");

        // 1. KYC verification rule
        if Self::is_rule_enabled(env.clone(), invoice_id.clone(), kyc_rule.clone()) {
            let from_kyc = Self::get_wallet_kyc(env.clone(), from.clone());
            let to_kyc = Self::get_wallet_kyc(env.clone(), to.clone());

            if !from_kyc || !to_kyc {
                let reason = Symbol::new(&env, "kyc_required");
                env.events().publish(
                    (
                        Symbol::new(&env, "TransferBlocked"),
                        from.clone(),
                        to.clone(),
                        invoice_id.clone(),
                    ),
                    reason.clone(),
                );
                return (false, reason);
            }
        }

        // 2. Jurisdiction block rule
        if Self::is_rule_enabled(env.clone(), invoice_id.clone(), jur_rule.clone()) {
            if let Some(from_jur) = Self::get_wallet_jurisdiction(env.clone(), from.clone()) {
                if Self::is_jurisdiction_blocked(env.clone(), invoice_id.clone(), from_jur) {
                    let reason = Symbol::new(&env, "jurisdiction_block");
                    env.events().publish(
                        (
                            Symbol::new(&env, "TransferBlocked"),
                            from.clone(),
                            to.clone(),
                            invoice_id.clone(),
                        ),
                        reason.clone(),
                    );
                    return (false, reason);
                }
            }

            if let Some(to_jur) = Self::get_wallet_jurisdiction(env.clone(), to.clone()) {
                if Self::is_jurisdiction_blocked(env.clone(), invoice_id.clone(), to_jur) {
                    let reason = Symbol::new(&env, "jurisdiction_block");
                    env.events().publish(
                        (
                            Symbol::new(&env, "TransferBlocked"),
                            from.clone(),
                            to.clone(),
                            invoice_id.clone(),
                        ),
                        reason.clone(),
                    );
                    return (false, reason);
                }
            }
        }

        // 3. Accreditation required rule
        if Self::is_rule_enabled(env.clone(), invoice_id.clone(), acc_rule.clone()) {
            let to_accredited = Self::get_wallet_accreditation(env.clone(), to.clone());
            if !to_accredited {
                let reason = Symbol::new(&env, "accreditation_required");
                env.events().publish(
                    (
                        Symbol::new(&env, "TransferBlocked"),
                        from.clone(),
                        to.clone(),
                        invoice_id.clone(),
                    ),
                    reason.clone(),
                );
                return (false, reason);
            }
        }

        (true, Symbol::new(&env, "allowed"))
    }

    /// Executes secondary market transfer after running check_transfer
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        invoice_id: Symbol,
        amount: i128,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        from.require_auth();

        let (allowed, _reason) =
            Self::check_transfer(env.clone(), from.clone(), to.clone(), invoice_id.clone());
        if !allowed {
            return Err(Error::TransferBlocked);
        }

        env.events().publish(
            (
                Symbol::new(&env, "TransferExecuted"),
                from,
                to,
                invoice_id,
            ),
            amount,
        );

        Ok(())
    }

    pub fn set_lockup(
        env: Env,
        invoice_id: Symbol,
        wallet: Address,
        duration_seconds: u64,
    ) -> Result<u64, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if duration_seconds == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::Lock(invoice_id, wallet));
            return Ok(env.ledger().timestamp());
        }
        let unlock_at = env
            .ledger()
            .timestamp()
            .checked_add(duration_seconds)
            .ok_or(Error::InvalidDuration)?;
        env.storage().persistent().set(
            &DataKey::Lock(invoice_id, wallet),
            &Lockup {
                unlock_at,
                expired_emitted: false,
            },
        );
        Ok(unlock_at)
    }

    pub fn is_transferable(env: Env, invoice_id: Symbol, wallet: Address) -> (bool, u64) {
        let lock: Option<Lockup> = env
            .storage()
            .persistent()
            .get(&DataKey::Lock(invoice_id, wallet));
        match lock {
            None => (true, 0),
            Some(lock) => (env.ledger().timestamp() >= lock.unlock_at, lock.unlock_at),
        }
    }

    pub fn validate_transfer(
        env: Env,
        invoice_id: Symbol,
        wallet: Address,
    ) -> Result<(), Error> {
        let key = DataKey::Lock(invoice_id.clone(), wallet.clone());
        let Some(mut lock) = env.storage().persistent().get::<_, Lockup>(&key) else {
            return Ok(());
        };
        if env.ledger().timestamp() < lock.unlock_at {
            return Err(Error::TransferLocked);
        }
        if !lock.expired_emitted {
            lock.expired_emitted = true;
            env.storage().persistent().set(&key, &lock);
            env.events().publish(
                (Symbol::new(&env, "LockupExpired"), invoice_id, wallet),
                lock.unlock_at,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_restriction_rules_and_transfer_check() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);
        let invoice_id = Symbol::new(&env, "INV-001");

        let contract_id = env.register(TransferRestriction, ());
        let client = TransferRestrictionClient::new(&env, &contract_id);

        client.initialize(&admin);

        // Initially no rules set -> transfer allowed
        let (allowed, reason) = client.check_transfer(&from, &to, &invoice_id);
        assert!(allowed);
        assert_eq!(reason, Symbol::new(&env, "allowed"));

        // Enable KYC rule
        client.set_restriction_rule(
            &invoice_id,
            &Symbol::new(&env, "kyc_required"),
            &true,
        );

        // Transfer should be blocked due to missing KYC
        let (allowed, reason) = client.check_transfer(&from, &to, &invoice_id);
        assert!(!allowed);
        assert_eq!(reason, Symbol::new(&env, "kyc_required"));

        // Verify from & to KYC
        client.set_wallet_kyc(&from, &true);
        client.set_wallet_kyc(&to, &true);

        // Now KYC passes
        let (allowed, _) = client.check_transfer(&from, &to, &invoice_id);
        assert!(allowed);

        // Enable Jurisdiction Block rule
        client.set_restriction_rule(
            &invoice_id,
            &Symbol::new(&env, "jurisdiction_block"),
            &true,
        );
        let jur_us = Symbol::new(&env, "US");
        let jur_uk = Symbol::new(&env, "UK");
        client.set_wallet_jurisdiction(&to, &jur_us);
        client.set_blocked_jurisdiction(&invoice_id, &jur_us, &true);

        // Blocked because recipient jurisdiction is blocked
        let (allowed, reason) = client.check_transfer(&from, &to, &invoice_id);
        assert!(!allowed);
        assert_eq!(reason, Symbol::new(&env, "jurisdiction_block"));

        // Change recipient jurisdiction to UK (unblocked)
        client.set_wallet_jurisdiction(&to, &jur_uk);
        let (allowed, _) = client.check_transfer(&from, &to, &invoice_id);
        assert!(allowed);

        // Enable Accreditation rule
        client.set_restriction_rule(
            &invoice_id,
            &Symbol::new(&env, "accreditation_required"),
            &true,
        );

        // Blocked because 'to' is not accredited
        let (allowed, reason) = client.check_transfer(&from, &to, &invoice_id);
        assert!(!allowed);
        assert_eq!(reason, Symbol::new(&env, "accreditation_required"));

        // Accredit recipient
        client.set_wallet_accreditation(&to, &true);
        let (allowed, _) = client.check_transfer(&from, &to, &invoice_id);
        assert!(allowed);

        // Successful transfer execution
        client.transfer(&from, &to, &invoice_id, &1000);
    }
}
