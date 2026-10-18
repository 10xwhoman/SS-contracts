#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};
pub use types::{CategoryInfo, Sector, StorageKey};

#[contract]
pub struct CategorisationRegistry;

#[contractimpl]
impl CategorisationRegistry {
    pub fn register_category(
        env: Env,
        issuer: Address,
        invoice_id: BytesN<32>,
        sector_raw: u32,
        sub_category: String,
    ) -> Result<(), Error> {
        issuer.require_auth();

        if sub_category.len() == 0 || sub_category.len() > 64 {
            return Err(Error::InvalidSubCategory);
        }

        let sector = match sector_raw {
            0 => Sector::Manufacturing,
            1 => Sector::Retail,
            2 => Sector::Logistics,
            3 => Sector::Healthcare,
            4 => Sector::Technology,
            5 => Sector::Other,
            _ => return Err(Error::InvalidSector),
        };

        if env
            .storage()
            .persistent()
            .has(&StorageKey::Category(invoice_id.clone()))
        {
            return Err(Error::AlreadyRegistered);
        }

        let registered_at = env.ledger().timestamp();
        let info = CategoryInfo {
            invoice_id: invoice_id.clone(),
            issuer: issuer.clone(),
            sector,
            sub_category: sub_category.clone(),
            registered_at,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Category(invoice_id.clone()), &info);

        let mut list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::SectorInvoices(sector))
            .unwrap_or(Vec::new(&env));

        list.push_back(invoice_id.clone());
        env.storage()
            .persistent()
            .set(&StorageKey::SectorInvoices(sector), &list);

        events::category_registered(&env, &invoice_id, &issuer, sector, &sub_category);

        Ok(())
    }

    pub fn get_category(env: Env, invoice_id: BytesN<32>) -> Result<CategoryInfo, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Category(invoice_id))
            .ok_or(Error::NotFound)
    }

    pub fn list_by_sector(
        env: Env,
        sector_raw: u32,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<BytesN<32>>, Error> {
        let sector = match sector_raw {
            0 => Sector::Manufacturing,
            1 => Sector::Retail,
            2 => Sector::Logistics,
            3 => Sector::Healthcare,
            4 => Sector::Technology,
            5 => Sector::Other,
            _ => return Err(Error::InvalidSector),
        };

        let list: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::SectorInvoices(sector))
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
}
