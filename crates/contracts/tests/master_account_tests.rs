use soroban_sdk::{
    testutils::{Address as _},
    Address, Env
};

use master_account::MasterAccountContract;

#[test]
fn test_initialize() {
    let env = Env::default();
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, MasterAccountContract);
    let client = MasterAccountContractClient::new(&env, &contract_id);

    client.initialize(&admin, &1);

    assert_eq!(client.get_admin(), admin);
}