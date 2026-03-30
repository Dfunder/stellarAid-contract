#![no_std]
use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

pub mod assets;
pub mod validation;
pub mod events;
pub mod donation;
pub mod storage_optimized;
pub mod donation_optimized;
pub mod storage_tests;
pub mod rbac;

#[cfg(test)]
mod integration_tests;

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn init(env: Env, admin: Address) {
        // Initialize global admin
        rbac::Rbac::set_admin(&env, &admin);

        // Initialize asset configuration
        assets::AssetConfig::init(&env, &admin);
    }

    pub fn ping(_env: Env) -> u32 {
        1
    }

    /// Record a donation and emit the DonationReceived event
    pub fn donate(
        env: Env,
        donor: Address,
        amount: i128,
        asset: String,
        project_id: String,
        tx_hash: String,
    ) -> i128 {
        // Check for duplicate transaction
        if donation::is_transaction_processed(&env, &tx_hash) {
            // Emit rejection event for duplicate
            events::DonationRejected {
                tx_hash: tx_hash.clone(),
                reason: String::from_str(&env, "Duplicate transaction hash"),
                timestamp: env.ledger().timestamp(),
            }
            .emit(&env);
            return 0;
        }

        // Validate donation data with detailed error handling
        match donation::validate_donation_with_error(&env, &donor, amount, &asset, &project_id) {
            Ok(()) => {},
            Err(_) => return 0,
        }

        // Get timestamp from ledger
        let timestamp = env.ledger().timestamp();

        // Mark transaction as processed BEFORE storing (prevents reentrancy)
        donation::mark_transaction_processed(&env, &tx_hash);

        // Store the donation on-chain
        let donation = donation::Donation::new(
            donor.clone(),
            amount,
            asset.clone(),
            project_id.clone(),
            timestamp,
            tx_hash.clone(),
        );
        
        // Get the index for this donation
        let index = donation::increment_donation_count(&env, &project_id) - 1;
        donation.store(&env, &project_id, index);

        // Emit the DonationReceived event with project_id
        events::DonationReceived {
            donor: donor.clone(),
            amount,
            asset: asset.clone(),
            project_id: project_id.clone(),
            timestamp,
        }
        .emit(&env);

        amount
    }

    /// Get all donations for a project
    pub fn get_donations(env: Env, project_id: String) -> soroban_sdk::Vec<Donation> {
        donation::get_donations_by_project(&env, &project_id)
    }

    // ===== Asset Management Functions (Admin Only) =====

    /// Add a new supported asset (admin only)
    pub fn add_supported_asset(
        env: Env,
        caller: Address,
        asset_code: String,
        contract_address: Address,
    ) -> Result<String, String> {
        // Ensure caller is admin
        rbac::Rbac::require_admin_auth(&env, &caller);

        assets::AssetConfig::add_asset(&env, &caller, &asset_code.to_string(), contract_address)
            .map(|_| asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Remove a supported asset (admin only)
    pub fn remove_supported_asset(env: Env, caller: Address, asset_code: String) -> Result<String, String> {
        assets::AssetConfig::remove_asset(&env, &caller, &asset_code.to_string())
            .map(|_| asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Update the asset admin (admin only)
    pub fn update_asset_admin(env: Env, caller: Address, new_admin: Address) -> Result<String, String> {
        assets::AssetConfig::update_admin(&env, &caller, &new_admin)
            .map(|_| String::from_str(&env, "Admin updated"))
            .map_err(|e| String::from_str(&env, e))
    }

    /// Get the list of all supported assets
    pub fn get_supported_assets(env: Env) -> soroban_sdk::Vec<String> {
        assets::AssetConfig::get_supported_assets(&env)
    }

    /// Check if an asset is supported
    pub fn is_asset_supported(env: Env, asset_code: String) -> bool {
        assets::AssetConfig::is_asset_supported(&env, &asset_code.to_string())
    }

    /// Get the current asset admin
    pub fn get_asset_admin(env: Env) -> Option<Address> {
        assets::AssetConfig::get_admin(&env)
    }

    /// Process a withdrawal and emit the WithdrawalProcessed event
    pub fn withdraw(
        env: Env,
        recipient: Address,
        amount: i128,
        asset: String,
    ) -> i128 {
        // Restricted to admin only
        rbac::Rbac::require_admin(&env);

        // Validate amount
        if amount <= 0 {
            panic!("Withdrawal amount must be positive");
        }

        // Resolve asset contract address
        let asset_code_str = asset.to_string();
        let asset_contract = assets::AssetConfig::get_contract_address(&env, &asset_code_str)
            .unwrap_or_else(|| panic!("Asset contract address not configured for {}", asset_code_str));

        // Initialize token client
        let token_client = token::Client::new(&env, &asset_contract);

        // Check contract balance
        let contract_address = env.current_contract_address();
        let balance = token_client.balance(&contract_address);
        if balance < amount {
            panic!("Insufficient contract balance for withdrawal");
        }

        // Execute transfer
        token_client.transfer(&contract_address, &recipient, &amount);

        // Emit the WithdrawalProcessed event
        events::WithdrawalProcessed {
            recipient: recipient.clone(),
            amount,
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        amount
    }
}

pub use donation::Donation;

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
        let valid_address = soroban_sdk::String::from_str(
            &env,
            "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37",
        );

        // Test that validation utilities are accessible
        let result = validate_stellar_address(&env, valid_address);
        assert!(result.is_ok());

        // Test boolean validation
        let valid_address2 = soroban_sdk::String::from_str(
            &env,
            "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA",
        );
        assert!(is_valid_stellar_address(&env, valid_address2));
    }

    // ===== Project ID Donation Mapping Tests =====

    #[test]
    fn test_donate_with_valid_project_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "proj-123");
        let tx_hash = String::from_str(&env, "abc123");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, amount);
    }

    #[test]
    fn test_donate_with_invalid_project_id_empty() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, ""); // Empty project ID
        let tx_hash = String::from_str(&env, "abc123");

        // Should return 0 for invalid project_id
        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_get_donations_groups_by_project_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let donor3 = Address::generate(&env);

        // Donate to project A
        let project_a = String::from_str(&env, "project-alpha");
        client.donate(&donor1, &1000i128, &String::from_str(&env, "XLM"), &project_a, &String::from_str(&env, "tx1"));
        client.donate(&donor2, &2000i128, &String::from_str(&env, "USDC"), &project_a, &String::from_str(&env, "tx2"));

        // Donate to project B
        let project_b = String::from_str(&env, "project-beta");
        client.donate(&donor3, &500i128, &String::from_str(&env, "XLM"), &project_b, &String::from_str(&env, "tx3"));

        // Get donations for project A
        let donations_a = client.get_donations(&project_a);
        assert_eq!(donations_a.len(), 2);

        // Get donations for project B
        let donations_b = client.get_donations(&project_b);
        assert_eq!(donations_b.len(), 1);
    }

    #[test]
    fn test_duplicate_transaction_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "unique-tx-hash-123");

        // First donation should succeed
        let result1 = client.donate(&donor, &1000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        assert_eq!(result1, 1000i128);

        // Second donation with same tx_hash should be rejected
        let result2 = client.donate(&donor, &2000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        assert_eq!(result2, 0);
    }

    // ===== Admin & Asset Management Tests =====

    #[test]
    fn test_admin_add_supported_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Admin adds BTC
        let btc_address = Address::generate(&env);
        let result = client.add_supported_asset(&admin, &String::from_str(&env, "BTC"), &btc_address);
        assert!(result.is_ok());

        assert!(client.is_asset_supported(&String::from_str(&env, "BTC")));
    }

    #[test]
    fn test_non_admin_cannot_add_asset() {
        let env = Env::default();
        // No mock_all_auths to test failure
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        client.init(&admin);

        let btc_address = Address::generate(&env);
        let result = client.add_supported_asset(&other, &String::from_str(&env, "BTC"), &btc_address);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw_success() {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        // Fund contract
        let amount = 1000i128;
        let token_admin = token::StellarAssetClient::new(&env, &asset_contract);
        token_admin.mint(&contract_id, &amount);

        let recipient = Address::generate(&env);
        let withdraw_amount = 500i128;
        let result = client.withdraw(&recipient, &withdraw_amount, &asset_code);

        assert_eq!(result, withdraw_amount);
        let token_client = token::Client::new(&env, &asset_contract);
        assert_eq!(token_client.balance(&contract_id), 500i128);
        assert_eq!(token_client.balance(&recipient), 500i128);
    }

    #[test]
    #[should_panic(expected = "Insufficient contract balance for withdrawal")]
    fn test_withdraw_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        let recipient = Address::generate(&env);
        client.withdraw(&recipient, &1000i128, &asset_code);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: caller is not admin")]
    fn test_withdraw_unauthorized() {
        let env = Env::default();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let recipient = Address::generate(&env);
        client.withdraw(&recipient, &1000i128, &String::from_str(&env, "USDC"));
    }
}
