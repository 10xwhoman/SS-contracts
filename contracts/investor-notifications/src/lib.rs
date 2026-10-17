#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Authorized(Address),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
}

#[contract]
pub struct InvestorNotifications;

fn require_authorized(env: &Env, caller: &Address) -> Result<(), Error> {
    if env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Admin)
        .is_none()
    {
        return Err(Error::NotInitialized);
    }
    if !env
        .storage()
        .persistent()
        .get(&DataKey::Authorized(caller.clone()))
        .unwrap_or(false)
    {
        return Err(Error::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

#[contractimpl]
impl InvestorNotifications {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Authorized(admin), &true);
        Ok(())
    }
    pub fn set_authorized_caller(env: Env, caller: Address, authorized: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Authorized(caller), &authorized);
        Ok(())
    }
    pub fn emit_settlement_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish(
            (Symbol::new(&env, "settlement"), wallet, invoice_id),
            amount,
        );
        Ok(())
    }
    pub fn emit_kyc_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        status: Symbol,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish((symbol_short!("kyc"), wallet), status);
        Ok(())
    }
    pub fn emit_auto_invest_notification(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        require_authorized(&env, &caller)?;
        env.events().publish(
            (Symbol::new(&env, "auto_invest"), wallet, invoice_id),
            amount,
        );
        Ok(())
    }
    pub fn is_authorized_caller(env: Env, caller: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Authorized(caller))
            .unwrap_or(false)
    }
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    #[test]
    fn only_authorized_callers_emit() {
        let env = Env::default();
        let contract = env.register(InvestorNotifications, ());
        let client = InvestorNotificationsClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let wallet = Address::generate(&env);
        client.initialize(&admin);
        assert!(client
            .try_emit_kyc_notification(&caller, &wallet, &symbol_short!("approved"))
            .is_err());
        client.set_authorized_caller(&caller, &true);
        client.emit_kyc_notification(&caller, &wallet, &symbol_short!("approved"));
    }
}
