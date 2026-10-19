use soroban_sdk::{contracttype, Address};

/// Referral tiers with associated commission rates.
///
/// Standard  –    0 – 49 999 referred volume → 20% fee share (2000 bps)
/// Plus      – 50 000 – 499 999              → 25% fee share (2500 bps)
/// Elite     – 500 000+                       → 30% fee share (3000 bps)
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReferralTier {
    Standard = 0,
    Plus = 1,
    Elite = 2,
}

/// Per-wallet referral state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralInfo {
    pub tier: ReferralTier,
    /// Cumulative referred investment volume (sum of all referred investments).
    pub cumulative_volume: i128,
    /// Volume threshold for the next tier, or `None` if already at Elite.
    pub next_tier_threshold: Option<i128>,
}

/// Contract-wide configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    /// Address of the bound referral contract (or escrow) that is the only
    /// caller allowed to accrue volume via `on_referral_reward_paid`.
    pub referral_contract: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Config,
    /// Persistent: cumulative referred volume for a wallet.
    Volume(Address),
    /// Persistent: current tier for a wallet.
    Tier(Address),
}

// ─── Tier thresholds ────────────────────────────────────────────────────────

pub const PLUS_THRESHOLD: i128 = 50_000;
pub const ELITE_THRESHOLD: i128 = 500_000;

pub fn commission_bps(tier: ReferralTier) -> i128 {
    match tier {
        ReferralTier::Standard => 2_000, // 20%
        ReferralTier::Plus => 2_500,     // 25%
        ReferralTier::Elite => 3_000,    // 30%
    }
}

pub fn tier_from_volume(volume: i128) -> ReferralTier {
    if volume >= ELITE_THRESHOLD {
        ReferralTier::Elite
    } else if volume >= PLUS_THRESHOLD {
        ReferralTier::Plus
    } else {
        ReferralTier::Standard
    }
}

pub fn next_threshold(tier: ReferralTier) -> Option<i128> {
    match tier {
        ReferralTier::Standard => Some(PLUS_THRESHOLD),
        ReferralTier::Plus => Some(ELITE_THRESHOLD),
        ReferralTier::Elite => None,
    }
}
