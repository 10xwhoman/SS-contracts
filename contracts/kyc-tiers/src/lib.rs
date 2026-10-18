//! KYC Tier Upgrade Contract (#576)
//!
//! Supports tiered verification levels (Basic, Verified, Accredited),
//! each unlocking higher per-invoice and cumulative investment limits.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum KYCTier {
    Basic,
    Verified,
    Accredited,
}

#[derive(Clone)]
#[contracttype]
pub struct InvestmentLimit {
    pub per_invoice: i128,
    pub cumulative: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct WalletKYC {
    pub wallet: Address,
    pub tier: KYCTier,
    pub cumulative_invested: i128,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    WalletKYC(Address),
}

#[contract]
pub struct KYCTiersContract;

#[contractimpl]
impl KYCTiersContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Set KYC tier for a wallet (admin-only)
    pub fn set_kyc_tier(env: Env, wallet: Address, tier: KYCTier) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let wallet_key = DataKey::WalletKYC(wallet.clone());
        
        // Get old tier if exists
        let old_tier = if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            existing.tier
        } else {
            KYCTier::Basic
        };

        let cumulative_invested = if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            existing.cumulative_invested
        } else {
            0
        };

        let wallet_kyc = WalletKYC {
            wallet: wallet.clone(),
            tier,
            cumulative_invested,
        };

        env.storage().persistent().set(&wallet_key, &wallet_kyc);

        env.events().publish(
            (symbol_short!("tier_upd"), wallet),
            (old_tier, tier),
        );
    }

    /// Get investment limits for a wallet's current tier
    pub fn get_investment_limit(env: Env, wallet: Address) -> InvestmentLimit {
        let wallet_key = DataKey::WalletKYC(wallet);
        
        let tier = if let Some(wallet_kyc) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            wallet_kyc.tier
        } else {
            KYCTier::Basic // Default tier
        };

        match tier {
            KYCTier::Basic => InvestmentLimit {
                per_invoice: 1000_0000000,    // 1k with 7 decimals
                cumulative: 5000_0000000,     // 5k with 7 decimals
            },
            KYCTier::Verified => InvestmentLimit {
                per_invoice: 10000_0000000,   // 10k with 7 decimals
                cumulative: 50000_0000000,    // 50k with 7 decimals
            },
            KYCTier::Accredited => InvestmentLimit {
                per_invoice: i128::MAX,       // Unlimited
                cumulative: i128::MAX,        // Unlimited
            },
        }
    }

    /// Validate investment amount against tier limits
    pub fn validate_investment(env: Env, wallet: Address, amount: i128) -> bool {
        let wallet_key = DataKey::WalletKYC(wallet.clone());
        
        let (tier, cumulative_invested) = if let Some(wallet_kyc) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            (wallet_kyc.tier, wallet_kyc.cumulative_invested)
        } else {
            (KYCTier::Basic, 0)
        };

        let limits = Self::get_investment_limit(env.clone(), wallet.clone());

        // Check per-invoice limit
        if amount > limits.per_invoice {
            panic!("LimitExceeded");
        }

        // Check cumulative limit
        if cumulative_invested + amount > limits.cumulative {
            panic!("LimitExceeded");
        }

        // Update cumulative invested
        let mut wallet_kyc = WalletKYC {
            wallet: wallet.clone(),
            tier,
            cumulative_invested: cumulative_invested + amount,
        };

        env.storage().persistent().set(&wallet_key, &wallet_kyc);

        true
    }

    /// Get wallet's current tier
    pub fn get_tier(env: Env, wallet: Address) -> KYCTier {
        let wallet_key = DataKey::WalletKYC(wallet);
        
        if let Some(wallet_kyc) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            wallet_kyc.tier
        } else {
            KYCTier::Basic
        }
    }

    /// Get wallet's cumulative invested amount
    pub fn get_cumulative_invested(env: Env, wallet: Address) -> i128 {
        let wallet_key = DataKey::WalletKYC(wallet);
        
        if let Some(wallet_kyc) = env
            .storage()
            .persistent()
            .get::<DataKey, WalletKYC>(&wallet_key)
        {
            wallet_kyc.cumulative_invested
        } else {
            0
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_set_kyc_tier() {
        let env = Env::default();
        let contract_id = env.register_contract(None, KYCTiersContract);
        let client = KYCTiersContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);
        client.set_kyc_tier(&wallet, &KYCTier::Verified);

        let tier = client.get_tier(&wallet);
        assert_eq!(tier, KYCTier::Verified);
    }

    #[test]
    fn test_investment_limits() {
        let env = Env::default();
        let contract_id = env.register_contract(None, KYCTiersContract);
        let client = KYCTiersContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);
        client.set_kyc_tier(&wallet, &KYCTier::Basic);

        let limits = client.get_investment_limit(&wallet);
        assert_eq!(limits.per_invoice, 1000_0000000);
        assert_eq!(limits.cumulative, 5000_0000000);
    }

    #[test]
    #[should_panic(expected = "LimitExceeded")]
    fn test_validate_investment_exceeds_limit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, KYCTiersContract);
        let client = KYCTiersContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);
        client.set_kyc_tier(&wallet, &KYCTier::Basic);

        // Try to invest more than per-invoice limit
        client.validate_investment(&wallet, &2000_0000000);
    }

    #[test]
    fn test_accredited_unlimited() {
        let env = Env::default();
        let contract_id = env.register_contract(None, KYCTiersContract);
        let client = KYCTiersContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);
        
        client.initialize(&admin);
        client.set_kyc_tier(&wallet, &KYCTier::Accredited);

        let limits = client.get_investment_limit(&wallet);
        assert_eq!(limits.per_invoice, i128::MAX);
        assert_eq!(limits.cumulative, i128::MAX);
    }
}
