use soroban_sdk::{contracttype, Address};

/// Loyalty tiers mirroring the platform loyalty-tier contract.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LoyaltyTier {
    Bronze = 0,
    Silver = 1,
    Gold = 2,
    Platinum = 3,
}

/// Contract-wide configuration stored in instance storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address allowed to update the contract.
    pub admin: Address,
    /// Address of the platform loyalty-tier contract (used as an oracle).
    pub loyalty_contract: Address,
    /// Platform fee in basis points before any discount is applied.
    pub platform_fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Config,
    /// Override tier stored per-wallet (used when loyalty contract is unavailable
    /// or for direct tier injection in tests).
    WalletTier(Address),
}
