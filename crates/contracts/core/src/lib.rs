#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env};

pub mod validation;

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn init(_env: Env, _admin: Address) {}

    pub fn ping(_env: Env) -> u32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init_and_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let result = client.ping();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_address_validation_integration() {
        use crate::validation::*;
        
        let env = Env::default();
        let valid_address = soroban_sdk::String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37");
        
        // Test that validation utilities are accessible
        let result = validate_stellar_address(&env, valid_address);
        assert!(result.is_ok());
        
        // Test boolean validation
        let valid_address2 = soroban_sdk::String::from_str(&env, "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA");
        assert!(is_valid_stellar_address(&env, valid_address2));
    }
}
