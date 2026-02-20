#![no_std]
use soroban_sdk::{contract, contractimpl, Env, Symbol};

#[contract]
pub struct StellarAidContract;

#[contractimpl]
impl StellarAidContract {
    pub fn hello(_env: Env, _to: Symbol) -> Symbol {
        soroban_sdk::symbol_short!("Hello")
    }
    
    pub fn get_greeting(_env: Env) -> Symbol {
        soroban_sdk::symbol_short!("Hi")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol, Env, symbol_short};

    #[test]
    fn test_hello() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let result = client.hello(&symbol_short!("World"));
        assert_eq!(result, symbol_short!("Hello"));
    }
    
    #[test]
    fn test_get_greeting() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let result = client.get_greeting();
        assert_eq!(result, symbol_short!("Hi"));
    }
}
