#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};
pub use types::StorageKey;

const MAX_WATCHLIST_SIZE: u32 = 50;

#[contract]
pub struct Watchlist;

#[contractimpl]
impl Watchlist {
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Operator, &operator);
        Ok(())
    }

    pub fn set_operator(env: Env, operator: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage().instance().set(&StorageKey::Operator, &operator);
        Ok(())
    }

    pub fn add_to_watchlist(env: Env, wallet: Address, invoice_id: BytesN<32>) -> Result<(), Error> {
        wallet.require_auth();

        let mut list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::Watchlist(wallet.clone()))
            .unwrap_or(Vec::new(&env));

        if list.len() >= MAX_WATCHLIST_SIZE {
            return Err(Error::WatchlistFull);
        }

        for i in 0..list.len() {
            if list.get(i).unwrap() == invoice_id {
                return Err(Error::AlreadyInWatchlist);
            }
        }

        list.push_back(invoice_id.clone());
        env.storage()
            .persistent()
            .set(&StorageKey::Watchlist(wallet.clone()), &list);

        // Also index watchers by invoice_id for state notification fanout
        let mut watchers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&StorageKey::InvoiceWatchers(invoice_id.clone()))
            .unwrap_or(Vec::new(&env));

        let mut already_watcher = false;
        for i in 0..watchers.len() {
            if watchers.get(i).unwrap() == wallet {
                already_watcher = true;
                break;
            }
        }
        if !already_watcher {
            watchers.push_back(wallet.clone());
            env.storage()
                .persistent()
                .set(&StorageKey::InvoiceWatchers(invoice_id.clone()), &watchers);
        }

        events::invoice_added_to_watchlist(&env, &wallet, &invoice_id);

        Ok(())
    }

    pub fn remove_from_watchlist(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        wallet.require_auth();

        let list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::Watchlist(wallet.clone()))
            .ok_or(Error::NotInWatchlist)?;

        let mut new_list = Vec::new(&env);
        let mut found = false;

        for i in 0..list.len() {
            let item = list.get(i).unwrap();
            if item == invoice_id {
                found = true;
            } else {
                new_list.push_back(item);
            }
        }

        if !found {
            return Err(Error::NotInWatchlist);
        }

        env.storage()
            .persistent()
            .set(&StorageKey::Watchlist(wallet.clone()), &new_list);

        // Remove from invoice watchers index
        if let Some(watchers) = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&StorageKey::InvoiceWatchers(invoice_id.clone()))
        {
            let mut new_watchers = Vec::new(&env);
            for i in 0..watchers.len() {
                let w = watchers.get(i).unwrap();
                if w != wallet {
                    new_watchers.push_back(w);
                }
            }
            env.storage().persistent().set(
                &StorageKey::InvoiceWatchers(invoice_id.clone()),
                &new_watchers,
            );
        }

        events::invoice_removed_from_watchlist(&env, &wallet, &invoice_id);

        Ok(())
    }

    pub fn get_watchlist(
        env: Env,
        wallet: Address,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<BytesN<32>>, Error> {
        let list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::Watchlist(wallet))
            .unwrap_or(Vec::new(&env));

        let mut result = Vec::new(&env);
        if page_size == 0 || list.is_empty() {
            return Ok(result);
        }

        let start_index = page.saturating_mul(page_size);
        if start_index >= list.len() {
            return Ok(result);
        }

        let end_index = (start_index + page_size).min(list.len());
        for i in start_index..end_index {
            result.push_back(list.get(i).unwrap());
        }

        Ok(result)
    }

    pub fn notify_state_change(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        old_state: u32,
        new_state: u32,
    ) -> Result<u32, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        let operator: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Operator)
            .ok_or(Error::NotInitialized)?;

        if caller != admin && caller != operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        let watchers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&StorageKey::InvoiceWatchers(invoice_id.clone()))
            .unwrap_or(Vec::new(&env));

        let count = watchers.len();
        for i in 0..count {
            let watcher = watchers.get(i).unwrap();
            events::watched_invoice_state_changed(
                &env,
                &invoice_id,
                &watcher,
                old_state,
                new_state,
            );
        }

        Ok(count)
    }
}
