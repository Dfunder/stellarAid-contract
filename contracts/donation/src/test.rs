#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_donation_and_balance_refresh_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    // Register our donation contract instance
    let contract_id = env.register_contract(None, DonationContract);
    let client = DonationContractClient::new(&env, &contract_id);

    // Generate dummy addresses representing the donor and the native token contract
    let donor = Address::generate(&env);
    let native_token_id = env.register_stellar_asset_contract(Address::generate(&env));
    let token_admin = token::StellarAssetClient::new(&env, &native_token_id);

    // Mint an initial native baseline balance to the donor account (e.g., 500.00 XLM = 5,000,000,000 stroops)
    let initial_balance: i128 = 5_000_000_000;
    token_admin.mint(&donor, &initial_balance);

    // Verify the pre-donation balance matches the setup target
    let current_balance = client.get_wallet_balance(&donor, &native_token_id);
    assert_eq!(current_balance, initial_balance);

    // Submit a donation of 100.00 XLM (1,000,000,000 stroops)
    let donation_amount: i128 = 1_000_000_000;
    
    // The action executes and returns the updated post-donation balance automatically
    let post_donation_balance = client.donate(&donor, &native_token_id, &donation_amount);

    // Task Requirement: Verify balance drops exactly by donation parameters (Should be 400.00 XLM remaining)
    assert_eq!(post_donation_balance, 4_000_000_000);
}