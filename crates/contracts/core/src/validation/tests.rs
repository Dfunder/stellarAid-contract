//! Comprehensive tests for Stellar address validation
//!
//! Tests cover all validation scenarios including:
//! - Valid addresses
//! - Invalid formats
//! - Invalid lengths
//! - Invalid characters
//! - Muxed account validation
//! - Error handling
//! - Edge cases

#![cfg(test)]

use soroban_sdk::{Env, String, Vec};
use crate::validation::{
    validate_stellar_address, 
    validate_standard_address, 
    validate_muxed_address,
    is_valid_stellar_address,
    validate_addresses,
    ValidationError,
    StellarAccount,
    StellarAddress,
    MuxedAddress
};

#[test]
fn test_valid_standard_addresses() {
    let env = Env::default();
    
    // Test various valid standard addresses
    let valid_addresses = vec![
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37",
        "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA",
        "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H",
        "GALOPAYIVBYBZX3JLCULH3CJ5XIOI3Z45J3AMM4TIMZDMTWI7P47D4JD",
    ];
    
    for address_str in valid_addresses {
        let address = String::from_str(&env, address_str);
        let result = validate_standard_address(&env, address);
        assert!(result.is_ok(), "Failed to validate address: {}", address_str);
        
        let validated_address = result.unwrap();
        assert_eq!(validated_address.as_str().as_str(), address_str);
    }
}

#[test]
fn test_valid_muxed_addresses() {
    let env = Env::default();
    
    // Test valid muxed addresses (69 characters starting with 'M')
    // Note: These are example formats - real muxed addresses would have proper base32 encoding
    let valid_muxed = vec![
        "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37ABCDEFGHIJKLMNOP", // 56 + 13 chars
        "MGAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA23456789ABCDEF",   // 56 + 13 chars
    ];
    
    for address_str in valid_muxed {
        let address = String::from_str(&env, address_str);
        let result = validate_muxed_address(&env, address);
        assert!(result.is_ok(), "Failed to validate muxed address: {}", address_str);
        
        let validated_address = result.unwrap();
        assert_eq!(validated_address.as_str().as_str(), &address_str[..56]);
        assert_eq!(validated_address.id(), 0); // Placeholder value
    }
}

#[test]
fn test_invalid_lengths() {
    let env = Env::default();
    
    // Test addresses with invalid lengths
    let invalid_lengths = vec![
        // Too short
        "",
        "G",
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3", // 55 chars
        "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37ABCDEFGHIJKLMN", // 68 chars
        
        // Too long
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37X", // 57 chars
        "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37ABCDEFGHIJKLMNOPQ", // 70 chars
    ];
    
    for address_str in invalid_lengths {
        let address = String::from_str(&env, address_str);
        let result = validate_stellar_address(&env, address);
        assert!(matches!(result, Err(ValidationError::InvalidLength)), 
                "Expected InvalidLength error for: {}", address_str);
    }
}

#[test]
fn test_invalid_prefixes() {
    let env = Env::default();
    
    // Test addresses with invalid prefixes
    let invalid_prefixes = vec![
        "ADQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37", // Starts with 'A'
        "XDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37", // Starts with 'X'
        "1DQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37", // Starts with '1'
        "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37", // Muxed prefix for standard validation
    ];
    
    for address_str in invalid_prefixes {
        let address = String::from_str(&env, address_str);
        let result = validate_standard_address(&env, address);
        assert!(matches!(result, Err(ValidationError::InvalidFormat)), 
                "Expected InvalidFormat error for: {}", address_str);
    }
}

#[test]
fn test_invalid_characters() {
    let env = Env::default();
    
    // Test addresses with invalid characters
    let invalid_chars = vec![
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W38", // Contains '8'
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3I", // Contains 'I'
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3O", // Contains 'O'
        "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3l", // Contains 'l' (lowercase L)
    ];
    
    for address_str in invalid_chars {
        let address = String::from_str(&env, address_str);
        let result = validate_stellar_address(&env, address);
        assert!(matches!(result, Err(ValidationError::InvalidCharacters)), 
                "Expected InvalidCharacters error for: {}", address_str);
    }
}

#[test]
fn test_batch_validation() {
    let env = Env::default();
    
    let addresses = Vec::from_array(&env, [
        String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37"),
        String::from_str(&env, "INVALID"),
        String::from_str(&env, "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA"),
    ]);
    
    let results = validate_addresses(&env, addresses);
    
    assert_eq!(results.len(), 3);
    assert!(results.get(0).unwrap().is_ok()); // First address valid
    assert!(results.get(1).unwrap().is_err()); // Second address invalid
    assert!(results.get(2).unwrap().is_ok()); // Third address valid
}

#[test]
fn test_boolean_validation() {
    let env = Env::default();
    
    // Valid addresses
    assert!(is_valid_stellar_address(&env, String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37")));
    assert!(is_valid_stellar_address(&env, String::from_str(&env, "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA")));
    
    // Invalid addresses
    assert!(!is_valid_stellar_address(&env, String::from_str(&env, "INVALID")));
    assert!(!is_valid_stellar_address(&env, String::from_str(&env, "")));
    assert!(!is_valid_stellar_address(&env, String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W3"))); // Too short
}

#[test]
fn test_error_messages() {
    let env = Env::default();
    
    // Test that each error type has a descriptive message
    let error_messages = vec![
        (ValidationError::EmptyAddress, "Address cannot be empty"),
        (ValidationError::InvalidLength, "Invalid address length - must be 56 characters for standard accounts or 69 for muxed accounts"),
        (ValidationError::InvalidFormat, "Invalid address format - must start with 'G' for standard accounts or 'M' for muxed accounts"),
        (ValidationError::InvalidCharacters, "Address contains invalid characters"),
    ];
    
    for (error, expected_message) in error_messages {
        assert_eq!(error.message(), expected_message);
    }
}

#[test]
fn test_address_type_conversion() {
    let env = Env::default();
    
    // Test standard address conversion
    let standard_addr = String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37");
    let result = validate_stellar_address(&env, standard_addr.clone()).unwrap();
    
    match result {
        StellarAccount::Standard(addr) => {
            assert_eq!(addr.as_str().as_str(), standard_addr.as_str());
        }
        StellarAccount::Muxed(_) => panic!("Expected standard account"),
    }
    
    // Test muxed address conversion
    let muxed_addr = String::from_str(&env, "MDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37ABCDEFGHIJKLMNOP");
    let result = validate_stellar_address(&env, muxed_addr.clone()).unwrap();
    
    match result {
        StellarAccount::Muxed(addr) => {
            assert_eq!(addr.as_str().as_str(), &muxed_addr.as_str()[..56]);
            assert_eq!(addr.id(), 0); // Placeholder
        }
        StellarAccount::Standard(_) => panic!("Expected muxed account"),
    }
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    
    // Test edge case: address with all valid base32 characters
    let all_valid_chars = String::from_str(&env, "ABCDEFGHIJKLMNOPQRSTUVWXYZ23456789ABCDEFGH"); // 56 chars
    // This should fail because '8' and '9' are not valid base32 chars
    let result = validate_stellar_address(&env, all_valid_chars);
    assert!(matches!(result, Err(ValidationError::InvalidCharacters)));
    
    // Test case sensitivity
    let lowercase_addr = String::from_str(&env, "gdqp2kpqgkihyjgxnuIyomharuarca7djt5fo2ffooky3b2wsqhg4w37");
    let result = validate_stellar_address(&env, lowercase_addr);
    assert!(matches!(result, Err(ValidationError::InvalidCharacters)));
}

#[test]
fn test_validation_compatibility() {
    let env = Env::default();
    
    // Test that the validation functions work together consistently
    let valid_address = String::from_str(&env, "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37");
    
    // All validation methods should agree on valid addresses
    assert!(is_valid_stellar_address(&env, valid_address.clone()));
    assert!(validate_standard_address(&env, valid_address.clone()).is_ok());
    assert!(validate_stellar_address(&env, valid_address.clone()).is_ok());
    
    // All validation methods should agree on invalid addresses
    let invalid_address = String::from_str(&env, "INVALID");
    assert!(!is_valid_stellar_address(&env, invalid_address.clone()));
    assert!(validate_standard_address(&env, invalid_address.clone()).is_err());
    assert!(validate_stellar_address(&env, invalid_address.clone()).is_err());
}