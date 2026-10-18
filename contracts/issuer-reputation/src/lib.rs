#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    AuthorisedCaller(Address),
    Tiers,
    IssuerData(Address),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TierThresholds {
    pub bronze: u32,
    pub silver: u32,
    pub gold: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct IssuerScoreData {
    pub score: u32,
    pub history_count: u32,
    pub on_time_count: u32,
    pub early_count: u32,
    pub late_count: u32,
    pub default_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct IssuerScoreInfo {
    pub score: u32,
    pub tier: Symbol,
    pub history_count: u32,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidEventType = 4,
    InvalidThresholds = 5,
}

const DEFAULT_BASELINE_SCORE: u32 = 500;
const MAX_SCORE: u32 = 1000;
const MIN_SCORE: u32 = 0;

#[contract]
pub struct IssuerReputation;

#[contractimpl]
impl IssuerReputation {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);

        // Default tier thresholds: Bronze >= 300, Silver >= 600, Gold >= 850
        let default_tiers = TierThresholds {
            bronze: 300,
            silver: 600,
            gold: 850,
        };
        env.storage().instance().set(&DataKey::Tiers, &default_tiers);

        Ok(())
    }

    /// Admin-only: configure tier thresholds for Bronze, Silver, and Gold
    pub fn set_issuer_tiers(
        env: Env,
        bronze: u32,
        silver: u32,
        gold: u32,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if bronze >= silver || silver >= gold || gold > MAX_SCORE {
            return Err(Error::InvalidThresholds);
        }

        let thresholds = TierThresholds {
            bronze,
            silver,
            gold,
        };
        env.storage().instance().set(&DataKey::Tiers, &thresholds);

        env.events().publish(
            (Symbol::new(&env, "TiersUpdated"),),
            (bronze, silver, gold),
        );

        Ok(())
    }

    /// Admin-only: authorize or revoke calling contracts (e.g., settlement and default contracts)
    pub fn set_authorised_caller(
        env: Env,
        caller: Address,
        authorised: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::AuthorisedCaller(caller.clone()), &authorised);

        env.events().publish(
            (Symbol::new(&env, "AuthorisedCallerUpdated"), caller),
            authorised,
        );

        Ok(())
    }

    pub fn is_authorised_caller(env: Env, caller: Address) -> bool {
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        if let Some(admin_addr) = admin {
            if admin_addr == caller {
                return true;
            }
        }
        env.storage()
            .persistent()
            .get(&DataKey::AuthorisedCaller(caller))
            .unwrap_or(false)
    }

    /// Called by authorized settlement and default contracts to record issuer outcome
    /// Supported event types: on_time_settlement, early_repayment, late_settlement, default
    pub fn update_issuer_score(
        env: Env,
        caller: Address,
        issuer: Address,
        event_type: Symbol,
    ) -> Result<(u32, u32), Error> {
        caller.require_auth();

        if !Self::is_authorised_caller(env.clone(), caller) {
            return Err(Error::Unauthorized);
        }

        let on_time = Symbol::new(&env, "on_time_settlement");
        let early = Symbol::new(&env, "early_repayment");
        let late = Symbol::new(&env, "late_settlement");
        let default_evt = Symbol::new(&env, "default");

        let mut data = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerData(issuer.clone()))
            .unwrap_or(IssuerScoreData {
                score: DEFAULT_BASELINE_SCORE,
                history_count: 0,
                on_time_count: 0,
                early_count: 0,
                late_count: 0,
                default_count: 0,
            });

        let old_score = data.score;
        let mut new_score = old_score;

        if event_type == on_time {
            new_score = old_score.saturating_add(30).min(MAX_SCORE);
            data.on_time_count = data.on_time_count.saturating_add(1);
        } else if event_type == early {
            new_score = old_score.saturating_add(60).min(MAX_SCORE);
            data.early_count = data.early_count.saturating_add(1);
        } else if event_type == late {
            new_score = old_score.saturating_sub(40).max(MIN_SCORE);
            data.late_count = data.late_count.saturating_add(1);
        } else if event_type == default_evt {
            new_score = old_score.saturating_sub(250).max(MIN_SCORE);
            data.default_count = data.default_count.saturating_add(1);
        } else {
            return Err(Error::InvalidEventType);
        }

        data.score = new_score;
        data.history_count = data.history_count.saturating_add(1);

        env.storage()
            .persistent()
            .set(&DataKey::IssuerData(issuer.clone()), &data);

        // Emits IssuerScoreUpdated event with issuer, old score, and new score
        env.events().publish(
            (Symbol::new(&env, "IssuerScoreUpdated"), issuer),
            (old_score, new_score),
        );

        Ok((old_score, new_score))
    }

    /// View function returning score, tier (Unrated/Bronze/Silver/Gold), and history count
    pub fn get_issuer_score(env: Env, issuer: Address) -> IssuerScoreInfo {
        let thresholds: TierThresholds = env
            .storage()
            .instance()
            .get(&DataKey::Tiers)
            .unwrap_or(TierThresholds {
                bronze: 300,
                silver: 600,
                gold: 850,
            });

        let data = env
            .storage()
            .persistent()
            .get(&DataKey::IssuerData(issuer))
            .unwrap_or(IssuerScoreData {
                score: DEFAULT_BASELINE_SCORE,
                history_count: 0,
                on_time_count: 0,
                early_count: 0,
                late_count: 0,
                default_count: 0,
            });

        let tier = if data.history_count == 0 {
            Symbol::new(&env, "Unrated")
        } else if data.score >= thresholds.gold {
            Symbol::new(&env, "Gold")
        } else if data.score >= thresholds.silver {
            Symbol::new(&env, "Silver")
        } else {
            Symbol::new(&env, "Bronze")
        };

        IssuerScoreInfo {
            score: data.score,
            tier,
            history_count: data.history_count,
        }
    }

    pub fn get_issuer_tiers(env: Env) -> Result<TierThresholds, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Tiers)
            .ok_or(Error::NotInitialized)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_issuer_reputation_scoring_and_tiers() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let settlement_contract = Address::generate(&env);
        let unauthorized_caller = Address::generate(&env);
        let issuer = Address::generate(&env);

        let contract_id = env.register(IssuerReputation, ());
        let client = IssuerReputationClient::new(&env, &contract_id);

        client.initialize(&admin);

        // Initial state before any events -> Unrated
        let initial_info = client.get_issuer_score(&issuer);
        assert_eq!(initial_info.score, 500);
        assert_eq!(initial_info.history_count, 0);
        assert_eq!(initial_info.tier, Symbol::new(&env, "Unrated"));

        // Unauthorized caller cannot update score
        let unauth_res = client.try_update_issuer_score(
            &unauthorized_caller,
            &issuer,
            &Symbol::new(&env, "on_time_settlement"),
        );
        assert!(unauth_res.is_err());

        // Authorize settlement contract
        client.set_authorised_caller(&settlement_contract, &true);

        // 1. on_time_settlement (+30 -> 530)
        let (old_s, new_s) = client.update_issuer_score(
            &settlement_contract,
            &issuer,
            &Symbol::new(&env, "on_time_settlement"),
        );
        assert_eq!(old_s, 500);
        assert_eq!(new_s, 530);

        let info = client.get_issuer_score(&issuer);
        assert_eq!(info.score, 530);
        assert_eq!(info.history_count, 1);
        assert_eq!(info.tier, Symbol::new(&env, "Bronze"));

        // 2. early_repayment (+60 -> 590)
        let (_, new_s2) = client.update_issuer_score(
            &settlement_contract,
            &issuer,
            &Symbol::new(&env, "early_repayment"),
        );
        assert_eq!(new_s2, 590);

        // Another early_repayment (+60 -> 650, crosses Silver threshold at 600)
        let (_, new_s3) = client.update_issuer_score(
            &settlement_contract,
            &issuer,
            &Symbol::new(&env, "early_repayment"),
        );
        assert_eq!(new_s3, 650);

        let info_silver = client.get_issuer_score(&issuer);
        assert_eq!(info_silver.tier, Symbol::new(&env, "Silver"));

        // 3. late_settlement (-40 -> 610)
        let (_, new_s4) = client.update_issuer_score(
            &settlement_contract,
            &issuer,
            &Symbol::new(&env, "late_settlement"),
        );
        assert_eq!(new_s4, 610);

        // 4. default (-250 -> 360)
        let (_, new_s5) = client.update_issuer_score(
            &settlement_contract,
            &issuer,
            &Symbol::new(&env, "default"),
        );
        assert_eq!(new_s5, 360);

        // Reconfigure tiers: set Bronze threshold to 400
        client.set_issuer_tiers(&400, &700, &900);
        let tiers = client.get_issuer_tiers();
        assert_eq!(tiers.bronze, 400);
        assert_eq!(tiers.silver, 700);
        assert_eq!(tiers.gold, 900);
    }
}
