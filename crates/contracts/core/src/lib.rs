#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env};

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
}
