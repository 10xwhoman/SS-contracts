//! Portfolio Snapshot contract for StellarSettle.
//!
//! Aggregates an investor's invoice fraction balances across every registered
//! invoice-token contract and records timestamped snapshots, so the backend
//! can serve portfolio value history charts.
//!
//! A snapshot records the wallet's balance in each registered invoice token at
//! one instant. Snapshots are kept for 90 days; expired ones are pruned
//! automatically when the next snapshot is taken for the same wallet, and can
//! also be pruned explicitly via `prune_snapshots`.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{SnapshotData, SnapshotInfo, TokenEntry};

use soroban_sdk::{
    contract, contractclient, contractimpl, Address, Env, Map, String, Symbol, Vec,
};

use errors::Error;

/// Maximum number of invoice-token contracts the registry tracks. Bounds the
/// per-snapshot cross-contract call fan-out.
const MAX_TRACKED_TOKENS: u32 = 200;

/// Snapshots are retained for 90 days.
const SNAPSHOT_RETENTION_SECS: u64 = 90 * 24 * 60 * 60;

/// Minimal cross-contract interface for reading an investor's fraction
/// balance from an invoice-token (SEP-41) contract.
#[contractclient(name = "FractionTokenClient")]
pub trait FractionTokenInterface {
    fn balance(env: Env, id: Address) -> i128;
}

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

/// Remove snapshots older than the 90-day retention window for a wallet.
/// Returns the number of snapshots removed.
fn prune_wallet(env: &Env, wallet: &Address, now: u64) -> u32 {
    let list = storage::get_snapshot_list(env, wallet);
    if list.is_empty() {
        return 0;
    }

    let mut kept = Vec::new(env);
    let mut removed = 0u32;
    for info in list.iter() {
        if now.saturating_sub(info.timestamp) > SNAPSHOT_RETENTION_SECS {
            storage::remove_snapshot(env, wallet, info.snapshot_id);
            removed += 1;
        } else {
            kept.push_back(info);
        }
    }

    if removed > 0 {
        storage::set_snapshot_list(env, wallet, &kept);
        events::snapshots_pruned(env, wallet, removed);
    }
    removed
}

#[contract]
pub struct PortfolioSnapshot;

#[contractimpl]
impl PortfolioSnapshot {
    /// Initialize the contract with the snapshot admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: register (or replace) the invoice-token contract that backs
    /// `invoice_id`. Registered tokens are queried on every snapshot.
    pub fn register_token(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        token: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &token)?;

        let mut tokens = storage::get_tokens(&env);
        let mut replaced = false;
        for i in 0..tokens.len() {
            let mut entry = tokens.get(i).unwrap();
            if entry.invoice_id == invoice_id {
                entry.token = token.clone();
                tokens.set(i, entry);
                replaced = true;
                break;
            }
        }
        if !replaced {
            if tokens.len() >= MAX_TRACKED_TOKENS {
                return Err(Error::TooManyTokens);
            }
            let entry = TokenEntry {
                invoice_id: invoice_id.clone(),
                token: token.clone(),
            };
            tokens.push_back(entry);
        }

        storage::set_tokens(&env, &tokens);
        events::token_registered(
            &env,
            &TokenEntry {
                invoice_id,
                token,
            },
        );
        Ok(())
    }

    /// Admin-only: stop tracking an invoice-token contract.
    pub fn remove_token(env: Env, admin: Address, invoice_id: Symbol) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let tokens = storage::get_tokens(&env);
        let mut kept = Vec::new(&env);
        let mut removed: Option<Address> = None;
        for entry in tokens.iter() {
            if entry.invoice_id == invoice_id {
                removed = Some(entry.token);
            } else {
                kept.push_back(entry);
            }
        }
        let token = removed.ok_or(Error::TokenNotFound)?;
        storage::set_tokens(&env, &kept);
        events::token_removed(&env, &invoice_id, &token);
        Ok(())
    }

    /// View: return every registered invoice-token contract.
    pub fn list_tokens(env: Env) -> Vec<TokenEntry> {
        storage::get_tokens(&env)
    }

    /// Record a snapshot of every invoice fraction balance held by `wallet`.
    ///
    /// Permissionless: the caller pays the fees and the recorded data is
    /// verifiable on-chain. Expired snapshots for the wallet are pruned first,
    /// so retention is enforced even without an explicit prune call.
    ///
    /// Returns the newly allocated snapshot id.
    pub fn take_portfolio_snapshot(env: Env, wallet: Address) -> Result<u64, Error> {
        ensure_non_zero_address(&env, &wallet)?;

        let now = env.ledger().timestamp();
        prune_wallet(&env, &wallet, now);

        let mut balances: Map<Symbol, i128> = Map::new(&env);
        for entry in storage::get_tokens(&env).iter() {
            let client = FractionTokenClient::new(&env, &entry.token);
            // Tokens that are no longer callable (missing/removed contracts)
            // are skipped rather than failing the whole snapshot.
            if let Ok(Ok(balance)) = client.try_balance(&wallet) {
                if balance > 0 {
                    balances.set(entry.invoice_id.clone(), balance);
                }
            }
        }

        let snapshot_id = storage::next_snapshot_id(&env, &wallet);
        let data = SnapshotData {
            snapshot_id,
            timestamp: now,
            balances,
        };
        storage::set_snapshot(&env, &wallet, snapshot_id, &data);

        let mut list = storage::get_snapshot_list(&env, &wallet);
        list.push_back(SnapshotInfo {
            snapshot_id,
            timestamp: now,
        });
        storage::set_snapshot_list(&env, &wallet, &list);

        events::snapshot_taken(&env, &wallet, snapshot_id, now);
        Ok(snapshot_id)
    }

    /// View: full balance map recorded for (wallet, snapshot_id).
    pub fn get_portfolio_snapshot(
        env: Env,
        wallet: Address,
        snapshot_id: u64,
    ) -> Result<SnapshotData, Error> {
        storage::get_snapshot(&env, &wallet, snapshot_id).ok_or(Error::SnapshotNotFound)
    }

    /// View: snapshot ids and timestamps for a wallet, oldest first.
    pub fn list_snapshots(env: Env, wallet: Address) -> Vec<SnapshotInfo> {
        storage::get_snapshot_list(&env, &wallet)
    }

    /// Admin-only: take one snapshot for each wallet in `wallets`. Intended to
    /// be invoked once per day by the backend with the list of active wallets.
    /// Returns the new snapshot id for each wallet, in input order.
    pub fn take_daily_snapshots(
        env: Env,
        admin: Address,
        wallets: Vec<Address>,
    ) -> Result<Vec<u64>, Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let mut ids = Vec::new(&env);
        for wallet in wallets.iter() {
            ids.push_back(Self::take_portfolio_snapshot(env.clone(), wallet)?);
        }
        Ok(ids)
    }

    /// Prune snapshots for a wallet that are older than the 90-day retention
    /// window. Permissionless: expired data can be removed by anyone.
    /// Returns the number of snapshots removed.
    pub fn prune_snapshots(env: Env, wallet: Address) -> Result<u32, Error> {
        ensure_non_zero_address(&env, &wallet)?;
        let now = env.ledger().timestamp();
        Ok(prune_wallet(&env, &wallet, now))
    }

    /// View: number of snapshots currently retained for a wallet.
    pub fn get_snapshot_count(env: Env, wallet: Address) -> u32 {
        storage::get_snapshot_list(&env, &wallet).len()
    }

    /// View: the retention window in seconds (90 days).
    pub fn get_retention_secs(_env: Env) -> u64 {
        SNAPSHOT_RETENTION_SECS
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
