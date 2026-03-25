#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String};

pub mod assets;
pub mod validation;
pub mod events;
pub mod donation;

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn init(_env: Env, _admin: Address) {}

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
        // Validate donation data
        if !donation::validate_donation(&donor, amount, &asset, &project_id) {
            return 0;
        }

        // Get timestamp from ledger
        let timestamp = env.ledger().timestamp();

        // Store the donation on-chain
        let donation = donation::Donation::new(
            donor.clone(),
            amount,
            asset.clone(),
            project_id.clone(),
            timestamp,
            tx_hash,
        );
        
        // Get the index for this donation
        let index = donation::increment_donation_count(&env, &project_id) - 1;
        donation.store(&env, &project_id, index);

        // Emit the DonationReceived event
        events::DonationReceived {
            donor: donor.clone(),
            amount,
            asset: asset.clone(),
            timestamp,
        }
        .emit(&env);

        amount
    }

    /// Get all donations for a project
    pub fn get_donations(env: Env, project_id: String) -> soroban_sdk::Vec<Donation> {
        donation::get_donations_by_project(&env, &project_id)
    }

    /// Process a withdrawal and emit the WithdrawalProcessed event
    pub fn withdraw(
        env: Env,
        recipient: Address,
        amount: i128,
        asset: String,
    ) -> i128 {
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
}
