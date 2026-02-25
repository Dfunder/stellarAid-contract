#![no_std]

use soroban_sdk::{
    contract, contractimpl, Address, Env, Vec
};

mod storage;
mod errors;
mod events;

use storage::DataKey;
use errors::ContractError;
use crate::validation::{validate_stellar_address, ValidationError};

#[contract]
pub struct MasterAccountContract;

#[contractimpl]
impl MasterAccountContract {

    // Initialize contract
    pub fn initialize(
        env: Env,
        admin: Address,
        threshold: u32,
    ) {
        if env.storage().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        admin.require_auth();

        env.storage().set(&DataKey::Admin, &admin);
        env.storage().set(&DataKey::Signers, &Vec::<Address>::new(&env));
        env.storage().set(&DataKey::Threshold, &threshold);
    }

    // Rotate admin
    pub fn rotate_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        env.storage().set(&DataKey::Admin, &new_admin);
        events::admin_rotated(&env, new_admin);
    }

    // Add signer (for multisig) with validation
    pub fn add_signer(env: Env, signer: Address) {
        let admin: Address = env.storage().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Validate the signer address format
        let signer_str = signer.to_string();
        if let Err(error) = validate_stellar_address(&env, signer_str) {
            error.panic(&env);
        }

        let mut signers: Vec<Address> =
            env.storage().get(&DataKey::Signers).unwrap();

        signers.push_back(signer.clone());

        env.storage().set(&DataKey::Signers, &signers);

        events::signer_added(&env, signer);
    }

    // Update threshold
    pub fn set_threshold(env: Env, threshold: u32) {
        let admin: Address = env.storage().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if threshold == 0 {
            panic_with_error!(&env, ContractError::InvalidThreshold);
        }

        env.storage().set(&DataKey::Threshold, &threshold);
    }

    // Getters
    pub fn get_admin(env: Env) -> Address {
        env.storage().get(&DataKey::Admin).unwrap()
    }

    pub fn get_threshold(env: Env) -> u32 {
        env.storage().get(&DataKey::Threshold).unwrap()
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        env.storage().get(&DataKey::Signers).unwrap()
    }
}