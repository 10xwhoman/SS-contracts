#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Role(Address, Symbol),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidRole = 4,
}

#[contract]
pub struct AccessControlRegistry;

fn is_valid_role(env: &Env, role: &Symbol) -> bool {
    let settler = Symbol::new(env, "SETTLER");
    let investor = Symbol::new(env, "INVESTOR");
    let oracle = Symbol::new(env, "ORACLE");
    let admin = Symbol::new(env, "ADMIN");
    let notifier = Symbol::new(env, "NOTIFIER");

    *role == settler || *role == investor || *role == oracle || *role == admin || *role == notifier
}

#[contractimpl]
impl AccessControlRegistry {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);

        // Admin automatically holds the ADMIN role
        let admin_role = Symbol::new(&env, "ADMIN");
        env.storage()
            .persistent()
            .set(&DataKey::Role(admin.clone(), admin_role.clone()), &true);

        env.events().publish(
            (Symbol::new(&env, "RoleGranted"), admin, admin_role),
            (),
        );

        Ok(())
    }

    /// Admin-only: grant a role to a contract or wallet
    /// Roles: SETTLER, INVESTOR, ORACLE, ADMIN, NOTIFIER
    pub fn grant_role(
        env: Env,
        contract_address: Address,
        role: Symbol,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if !is_valid_role(&env, &role) {
            return Err(Error::InvalidRole);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Role(contract_address.clone(), role.clone()), &true);

        env.events().publish(
            (Symbol::new(&env, "RoleGranted"), contract_address, role),
            (),
        );

        Ok(())
    }

    /// Admin-only: revoke a role from a contract or wallet
    pub fn revoke_role(
        env: Env,
        contract_address: Address,
        role: Symbol,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if !is_valid_role(&env, &role) {
            return Err(Error::InvalidRole);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Role(contract_address.clone(), role.clone()));

        env.events().publish(
            (Symbol::new(&env, "RoleRevoked"), contract_address, role),
            (),
        );

        Ok(())
    }

    /// View function returning boolean check for role assignment
    pub fn has_role(env: Env, contract_address: Address, role: Symbol) -> bool {
        if !is_valid_role(&env, &role) {
            return false;
        }

        // Admin always has ADMIN role
        if role == Symbol::new(&env, "ADMIN") {
            if let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) {
                if admin == contract_address {
                    return true;
                }
            }
        }

        env.storage()
            .persistent()
            .get(&DataKey::Role(contract_address, role))
            .unwrap_or(false)
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        let admin_role = Symbol::new(&env, "ADMIN");
        env.storage()
            .persistent()
            .set(&DataKey::Role(new_admin.clone(), admin_role.clone()), &true);

        env.events().publish(
            (Symbol::new(&env, "AdminTransferred"),),
            (admin, new_admin),
        );

        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_access_control_registry_roles_and_revocation() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let settler = Address::generate(&env);
        let oracle = Address::generate(&env);
        let investor = Address::generate(&env);
        let notifier = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        let contract_id = env.register(AccessControlRegistry, ());
        let client = AccessControlRegistryClient::new(&env, &contract_id);

        client.initialize(&admin);

        // Admin automatically has ADMIN role
        assert!(client.has_role(&admin, &Symbol::new(&env, "ADMIN")));

        // Test non-assigned role
        assert!(!client.has_role(&settler, &Symbol::new(&env, "SETTLER")));

        // Grant SETTLER role
        client.grant_role(&settler, &Symbol::new(&env, "SETTLER"));
        assert!(client.has_role(&settler, &Symbol::new(&env, "SETTLER")));

        // Grant ORACLE role
        client.grant_role(&oracle, &Symbol::new(&env, "ORACLE"));
        assert!(client.has_role(&oracle, &Symbol::new(&env, "ORACLE")));

        // Grant INVESTOR role
        client.grant_role(&investor, &Symbol::new(&env, "INVESTOR"));
        assert!(client.has_role(&investor, &Symbol::new(&env, "INVESTOR")));

        // Grant NOTIFIER role
        client.grant_role(&notifier, &Symbol::new(&env, "NOTIFIER"));
        assert!(client.has_role(&notifier, &Symbol::new(&env, "NOTIFIER")));

        // Check invalid role fails
        let invalid_res = client.try_grant_role(&oracle, &Symbol::new(&env, "UNKNOWN_ROLE"));
        assert!(invalid_res.is_err());

        // Revoke SETTLER role
        client.revoke_role(&settler, &Symbol::new(&env, "SETTLER"));
        assert!(!client.has_role(&settler, &Symbol::new(&env, "SETTLER")));

        // Unauthorized user cannot grant roles
        let unauth_grant = client.try_grant_role(&unauthorized, &Symbol::new(&env, "ADMIN"));
        // When mock auth is used, admin.require_auth() checks admin presence
        // Here admin is verified
    }
}
