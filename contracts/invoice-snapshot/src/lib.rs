//! Investor Position Snapshot contract for StellarSettle (issue #527).
//!
//! Records every holder's balance for an invoice at the current ledger,
//! enabling the backend to reconstruct portfolio value history for charting.
//! Snapshots are taken daily by the admin, kept for a 90-day retention window,
//! and pruned automatically on the next snapshot for that invoice (or on
//! demand via `prune_snapshots`). Expired snapshots are also invisible to the
//! `get_snapshot` / `get_investor_snapshots` reads, so the retention policy
//! holds even before physical pruning runs.
//!
//! A snapshot also serves as the basis for pro-rata airdrop or bonus
//! distributions (`distribute_from_snapshot`), so holders never need to be
//! enumerated in real time.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

pub use types::{HolderBalance, InvestorSnapshot, Snapshot, SnapshotRef};

use errors::Error;

/// Snapshot retention window: 90 days in ledger seconds.
const RETENTION_SECS: u64 = 90 * 24 * 60 * 60;

/// Upper bound on holders per invoice, bounding snapshot size and prune cost.
const MAX_HOLDERS: u32 = 200;

#[contract]
pub struct InvoiceSnapshot;

#[contractimpl]
impl InvoiceSnapshot {
    /// Initialize the contract with the admin that triggers daily snapshots
    /// and maintains the holder registry.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        Ok(())
    }

    /// Admin-only: set (or update, or clear with `0`) a holder's balance for an
    /// invoice. The registry is what `take_snapshot` captures.
    pub fn set_holder_balance(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        holder: Address,
        amount: i128,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::ensure_admin(&env, &admin)?;
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }

        let existing = storage::get_holder_balance(&env, &invoice_id, &holder);
        storage::set_holder_balance(&env, &invoice_id, &holder, amount);

        if amount == 0 {
            if existing > 0 {
                let holders = storage::get_holders(&env, &invoice_id);
                let mut kept = Vec::new(&env);
                for h in holders.iter() {
                    if h != holder {
                        kept.push_back(h);
                    }
                }
                storage::set_holders(&env, &invoice_id, &kept);
            }
        } else if existing == 0 {
            let holders = storage::get_holders(&env, &invoice_id);
            let mut present = false;
            for h in holders.iter() {
                if h == holder {
                    present = true;
                    break;
                }
            }
            if !present {
                if holders.len() >= MAX_HOLDERS {
                    return Err(Error::TooManyHolders);
                }
                let mut updated = holders;
                updated.push_back(holder.clone());
                storage::set_holders(&env, &invoice_id, &updated);
            }
        }
        Ok(())
    }

    /// Admin-only: record every holder balance for `invoice_id` at the current
    /// ledger. Returns the new snapshot id (starting at 1). Expired snapshots
    /// past the 90-day window are pruned first.
    pub fn take_snapshot(env: Env, admin: Address, invoice_id: Symbol) -> Result<u32, Error> {
        admin.require_auth();
        Self::ensure_admin(&env, &admin)?;

        // Enforce retention before appending: expired snapshots never linger
        // past the retention window once a new snapshot is taken.
        Self::prune_expired(&env, &invoice_id)?;

        let holders = storage::get_holders(&env, &invoice_id);
        let mut balances = Vec::new(&env);
        let mut total_balance: i128 = 0;
        for holder in holders.iter() {
            let amount = storage::get_holder_balance(&env, &invoice_id, &holder);
            if amount > 0 {
                total_balance = total_balance.checked_add(amount).ok_or(Error::Overflow)?;
                balances.push_back(HolderBalance {
                    holder: holder.clone(),
                    amount,
                });
            }
        }

        let snapshot_id = storage::get_last_snapshot_id(&env, &invoice_id)
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        storage::set_last_snapshot_id(&env, &invoice_id, snapshot_id);

        let ledger_sequence = env.ledger().sequence();
        let timestamp = env.ledger().timestamp();
        let snapshot = Snapshot {
            snapshot_id,
            invoice_id: invoice_id.clone(),
            ledger_sequence,
            timestamp,
            balances: balances.clone(),
            total_balance,
        };
        storage::set_snapshot(&env, &invoice_id, snapshot_id, &snapshot);

        let mut index = storage::get_snapshot_index(&env, &invoice_id);
        index.push_back(snapshot_id);
        storage::set_snapshot_index(&env, &invoice_id, &index);

        // Index the snapshot under every holder for cross-invoice history.
        for entry in balances.iter() {
            let mut wallet_index = storage::get_wallet_index(&env, &entry.holder);
            wallet_index.push_back(InvestorSnapshot {
                invoice_id: invoice_id.clone(),
                snapshot_id,
                ledger_sequence,
                timestamp,
                balance: entry.amount,
            });
            storage::set_wallet_index(&env, &entry.holder, &wallet_index);
        }

        events::snapshot_taken(&env, invoice_id, snapshot_id, ledger_sequence);
        Ok(snapshot_id)
    }

    /// View: return a snapshot's full balance map.
    ///
    /// Snapshots older than the 90-day retention window are reported as
    /// `SnapshotNotFound` even if storage pruning has not run yet.
    pub fn get_snapshot(env: Env, invoice_id: Symbol, snapshot_id: u32) -> Result<Snapshot, Error> {
        let snapshot =
            storage::get_snapshot(&env, &invoice_id, snapshot_id).ok_or(Error::SnapshotNotFound)?;
        if Self::snapshot_expired(env.ledger().timestamp(), snapshot.timestamp) {
            return Err(Error::SnapshotNotFound);
        }
        Ok(snapshot)
    }

    /// View: a wallet's balance inside a snapshot (0 if it held no position).
    ///
    /// Reads the immutable snapshot record, so balance changes made after the
    /// snapshot was taken never affect the result.
    pub fn get_snapshot_balance(
        env: Env,
        invoice_id: Symbol,
        snapshot_id: u32,
        wallet: Address,
    ) -> Result<i128, Error> {
        let snapshot = Self::get_snapshot(env, invoice_id, snapshot_id)?;
        for entry in snapshot.balances.iter() {
            if entry.holder == wallet {
                return Ok(entry.amount);
            }
        }
        Ok(0)
    }

    /// View: every live snapshot id and timestamp for an invoice, oldest first.
    pub fn list_snapshots(env: Env, invoice_id: Symbol) -> Vec<SnapshotRef> {
        let now = env.ledger().timestamp();
        let mut refs = Vec::new(&env);
        for snapshot_id in storage::get_snapshot_index(&env, &invoice_id).iter() {
            if let Some(snapshot) = storage::get_snapshot(&env, &invoice_id, snapshot_id) {
                if !Self::snapshot_expired(now, snapshot.timestamp) {
                    refs.push_back(SnapshotRef {
                        snapshot_id,
                        timestamp: snapshot.timestamp,
                    });
                }
            }
        }
        refs
    }

    /// Admin-only: route `total_amount` of `token` pro-rata to every holder in a
    /// snapshot, weighted by the balances recorded at snapshot time. The funds
    /// are pulled from `admin`. Each share is floored, so rounding dust stays
    /// with the admin. Returns the amount actually distributed.
    pub fn distribute_from_snapshot(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        snapshot_id: u32,
        token: Address,
        total_amount: i128,
    ) -> Result<i128, Error> {
        admin.require_auth();
        Self::ensure_admin(&env, &admin)?;
        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let snapshot = Self::get_snapshot(env.clone(), invoice_id.clone(), snapshot_id)?;
        if snapshot.total_balance <= 0 {
            return Err(Error::NothingToDistribute);
        }

        let token_client = token::Client::new(&env, &token);
        let mut distributed: i128 = 0;
        for entry in snapshot.balances.iter() {
            let share = total_amount
                .checked_mul(entry.amount)
                .ok_or(Error::Overflow)?
                / snapshot.total_balance;
            if share > 0 {
                token_client.transfer(&admin, &entry.holder, &share);
                distributed = distributed.checked_add(share).ok_or(Error::Overflow)?;
            }
        }

        events::snapshot_distributed(
            &env,
            invoice_id,
            snapshot_id,
            token,
            total_amount,
            distributed,
        );
        Ok(distributed)
    }

    /// View: every live snapshot reference for `wallet`, across all invoices
    /// it holds positions in, oldest first. Expired entries are filtered out.
    pub fn get_investor_snapshots(env: Env, wallet: Address) -> Vec<InvestorSnapshot> {
        let now = env.ledger().timestamp();
        let mut live = Vec::new(&env);
        for entry in storage::get_wallet_index(&env, &wallet).iter() {
            if !Self::snapshot_expired(now, entry.timestamp) {
                live.push_back(entry.clone());
            }
        }
        live
    }

    /// Remove every snapshot older than the 90-day retention window for an
    /// invoice. Callable by anyone (retention is a storage-hygiene policy, not
    /// a privileged action). Returns the number of snapshots pruned.
    pub fn prune_snapshots(env: Env, invoice_id: Symbol) -> Result<u32, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Self::prune_expired(&env, &invoice_id)
    }

    /// View: live snapshot ids for an invoice, oldest first.
    pub fn get_snapshot_ids(env: Env, invoice_id: Symbol) -> Vec<u32> {
        storage::get_snapshot_index(&env, &invoice_id)
    }

    /// View: the most recently assigned snapshot id (0 when none has been taken).
    pub fn get_latest_snapshot_id(env: Env, invoice_id: Symbol) -> u32 {
        storage::get_last_snapshot_id(&env, &invoice_id)
    }

    /// View: a holder's current registered balance for an invoice.
    pub fn get_holder_balance(env: Env, invoice_id: Symbol, holder: Address) -> i128 {
        storage::get_holder_balance(&env, &invoice_id, &holder)
    }

    /// View: the current holder registry for an invoice.
    pub fn get_holders(env: Env, invoice_id: Symbol) -> Vec<Address> {
        storage::get_holders(&env, &invoice_id)
    }

    /// View: the configured admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    fn ensure_admin(env: &Env, caller: &Address) -> Result<(), Error> {
        let admin = storage::get_admin(env).ok_or(Error::NotInit)?;
        if admin != *caller {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    fn snapshot_expired(now: u64, timestamp: u64) -> bool {
        now > timestamp.saturating_add(RETENTION_SECS)
    }

    /// Delete every expired snapshot for an invoice, removing its wallet-index
    /// references and its id from the invoice index. Returns the prune count.
    fn prune_expired(env: &Env, invoice_id: &Symbol) -> Result<u32, Error> {
        let now = env.ledger().timestamp();
        let index = storage::get_snapshot_index(env, invoice_id);
        let mut kept = Vec::new(env);
        let mut pruned: u32 = 0;

        for snapshot_id in index.iter() {
            match storage::get_snapshot(env, invoice_id, snapshot_id) {
                Some(snapshot) if Self::snapshot_expired(now, snapshot.timestamp) => {
                    for entry in snapshot.balances.iter() {
                        let wallet_index = storage::get_wallet_index(env, &entry.holder);
                        let mut kept_entries = Vec::new(env);
                        for item in wallet_index.iter() {
                            let matches =
                                item.invoice_id == *invoice_id && item.snapshot_id == snapshot_id;
                            if !matches {
                                kept_entries.push_back(item.clone());
                            }
                        }
                        storage::set_wallet_index(env, &entry.holder, &kept_entries);
                    }
                    storage::remove_snapshot(env, invoice_id, snapshot_id);
                    pruned += 1;
                }
                Some(_) => kept.push_back(snapshot_id),
                None => {} // Dangling id; drop it from the index.
            }
        }

        if pruned > 0 {
            storage::set_snapshot_index(env, invoice_id, &kept);
            events::snapshots_pruned(env, invoice_id.clone(), pruned, kept.len());
        }
        Ok(pruned)
    }
}

#[cfg(test)]
mod test;
