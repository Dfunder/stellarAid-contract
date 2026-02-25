//! Example usage of Stellar address validation utilities
//!
//! This file demonstrates various ways to use the validation utilities
//! in different contract scenarios.

use soroban_sdk::{contract, contractimpl, Address, Env, String};
use stellaraid_core::validation::{
    validate_stellar_address,
    validate_standard_address,
    validate_muxed_address,
    is_valid_stellar_address,
    ValidationError,
    StellarAccount,
    StellarAddress,
    MuxedAddress
};

/// Example contract demonstrating address validation usage
#[contract]
pub struct AddressValidationExample;

#[contractimpl]
impl AddressValidationExample {
    /// Example 1: Basic address validation
    pub fn validate_donor_address(env: Env, donor: Address) -> bool {
        let address_str = donor.to_string();
        is_valid_stellar_address(&env, address_str)
    }

    /// Example 2: Strict standard address validation
    pub fn add_multisig_signer(env: Env, signer: Address) -> Result<(), ValidationError> {
        let signer_str = signer.to_string();
        // Only accept standard addresses for multisig
        validate_standard_address(&env, signer_str)?;
        // ... add signer to multisig
        Ok(())
    }

    /// Example 3: Handle both standard and muxed accounts
    pub fn process_payment(env: Env, recipient: Address) -> Result<String, ValidationError> {
        let recipient_str = recipient.to_string();
        match validate_stellar_address(&env, recipient_str)? {
            StellarAccount::Standard(addr) => {
                // Process standard payment
                Ok(format!("Processed payment to standard account: {}", addr.as_str()))
            }
            StellarAccount::Muxed(muxed_addr) => {
                // Process muxed payment
                Ok(format!(
                    "Processed payment to muxed account: {} with ID: {}", 
                    muxed_addr.as_str(), 
                    muxed_addr.id()
                ))
            }
        }
    }

    /// Example 4: Batch address validation
    pub fn validate_donors(env: Env, donors: Vec<Address>) -> Vec<bool> {
        let mut results = Vec::new(&env);
        for donor in donors.iter() {
            let is_valid = Self::validate_donor_address(env.clone(), donor.clone());
            results.push_back(is_valid);
        }
        results
    }

    /// Example 5: Validation with custom error handling
    pub fn validate_and_log(env: Env, address: Address) -> Result<(), String> {
        let address_str = address.to_string();
        
        match validate_stellar_address(&env, address_str) {
            Ok(_) => {
                // Address is valid
                Ok(())
            }
            Err(ValidationError::InvalidLength) => {
                Err("Address has invalid length - must be 56 or 69 characters".to_string())
            }
            Err(ValidationError::InvalidFormat) => {
                Err("Address must start with 'G' (standard) or 'M' (muxed)".to_string())
            }
            Err(ValidationError::InvalidCharacters) => {
                Err("Address contains invalid characters - only base32 allowed".to_string())
            }
            Err(error) => {
                Err(format!("Address validation failed: {}", error.message()))
            }
        }
    }
}

/// Example usage functions (not contract methods)
pub fn example_usage() {
    let env = Env::default();
    
    // Example 1: Valid addresses
    let valid_standard = String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37");
    let valid_muxed = String::from_str(&env, "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37ABCDEFGHIJKLMNOP");
    
    // Example 2: Invalid addresses
    let invalid_length = String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3"); // 55 chars
    let invalid_prefix = String::from_str(&env, "ADQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37"); // Starts with 'A'
    let invalid_chars = String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W38"); // Contains '8'
    
    // Test valid addresses
    assert!(is_valid_stellar_address(&env, valid_standard.clone()));
    assert!(is_valid_stellar_address(&env, valid_muxed.clone()));
    
    // Test invalid addresses
    assert!(!is_valid_stellar_address(&env, invalid_length));
    assert!(!is_valid_stellar_address(&env, invalid_prefix));
    assert!(!is_valid_stellar_address(&env, invalid_chars));
    
    println!("All validation examples passed!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address, Vec};

    #[test]
    fn test_example_contract() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AddressValidationExample);
        let client = AddressValidationExampleClient::new(&env, &contract_id);

        // Test with valid address
        let valid_address = Address::from_string(&String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37"));
        let is_valid = client.validate_donor_address(&valid_address);
        assert!(is_valid);

        // Test with invalid address
        let invalid_address = Address::from_string(&String::from_str(&env, "INVALID"));
        let is_valid = client.validate_donor_address(&invalid_address);
        assert!(!is_valid);
    }

    #[test]
    fn test_batch_validation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AddressValidationExample);
        let client = AddressValidationExampleClient::new(&env, &contract_id);

        let addresses = Vec::from_array(&env, [
            Address::from_string(&String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37")),
            Address::from_string(&String::from_str(&env, "INVALID")),
            Address::from_string(&String::from_str(&env, "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA")),
        ]);

        let results = client.validate_donors(&addresses);
        assert_eq!(results.len(), 3);
        assert!(results.get(0).unwrap());  // Valid
        assert!(!results.get(1).unwrap()); // Invalid
        assert!(results.get(2).unwrap());  // Valid
    }
}