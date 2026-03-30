#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::{InputValidator, ValidationError};

    #[test]
    fn test_validate_stellar_address() {
        // Valid addresses
        assert!(InputValidator::validate_stellar_address("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K").is_ok());
        assert!(InputValidator::validate_stellar_address("GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R").is_ok());

        // Invalid addresses
        assert!(InputValidator::validate_stellar_address("").is_err());
        assert!(InputValidator::validate_stellar_address("ABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K").is_err());
        assert!(InputValidator::validate_stellar_address("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6").is_err());
        assert!(InputValidator::validate_stellar_address("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6KK").is_err());
    }

    #[test]
    fn test_validate_amount() {
        // Valid amounts
        assert!(InputValidator::validate_amount("10").is_ok());
        assert!(InputValidator::validate_amount("10.5").is_ok());
        assert!(InputValidator::validate_amount("0.0000001").is_ok());
        assert!(InputValidator::validate_amount("1000000000").is_ok());

        // Invalid amounts
        assert!(InputValidator::validate_amount("").is_err());
        assert!(InputValidator::validate_amount("0").is_err());
        assert!(InputValidator::validate_amount("-10").is_err());
        assert!(InputValidator::validate_amount("10.12345678").is_err()); // Too many decimals
        assert!(InputValidator::validate_amount("abc").is_err());
        assert!(InputValidator::validate_amount("2000000000").is_err()); // Too large
    }

    #[test]
    fn test_validate_asset_code() {
        // Valid asset codes
        assert!(InputValidator::validate_asset_code("XLM").is_ok());
        assert!(InputValidator::validate_asset_code("USDC").is_ok());
        assert!(InputValidator::validate_asset_code("EURT").is_ok());
        assert!(InputValidator::validate_asset_code("ABC12").is_ok());

        // Invalid asset codes
        assert!(InputValidator::validate_asset_code("").is_err());
        assert!(InputValidator::validate_asset_code("ASSETCODETOOLONG").is_err());
        assert!(InputValidator::validate_asset_code("USD@").is_err());
    }

    #[test]
    fn test_validate_contract_id() {
        // Valid contract IDs
        assert!(InputValidator::validate_contract_id("CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3").is_ok());
        assert!(InputValidator::validate_contract_id("CB2EHQKPEWQWKLFRYIRLQYUVJGHZPXFL5FXYE7Y3EFAKQFCENKZQAAAA").is_ok());

        // Invalid contract IDs
        assert!(InputValidator::validate_contract_id("").is_err());
        assert!(InputValidator::validate_contract_id("GA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3").is_err());
        assert!(InputValidator::validate_contract_id("CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA").is_err());
    }

    #[test]
    fn test_validate_transaction_hash() {
        // Valid transaction hashes
        assert!(InputValidator::validate_transaction_hash("a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456").is_ok());

        // Invalid transaction hashes
        assert!(InputValidator::validate_transaction_hash("").is_err());
        assert!(InputValidator::validate_transaction_hash("a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345").is_err()); // Too short
        assert!(InputValidator::validate_transaction_hash("a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567").is_err()); // Too long
        assert!(InputValidator::validate_transaction_hash("g1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456").is_err()); // Invalid hex
    }

    #[test]
    fn test_validate_network() {
        // Valid networks
        assert!(InputValidator::validate_network("testnet").is_ok());
        assert!(InputValidator::validate_network("mainnet").is_ok());
        assert!(InputValidator::validate_network("sandbox").is_ok());
        assert!(InputValidator::validate_network("public").is_ok());
        assert!(InputValidator::validate_network("future").is_ok());

        // Invalid networks
        assert!(InputValidator::validate_network("").is_err());
        assert!(InputValidator::validate_network("invalid").is_err());
        assert!(InputValidator::validate_network("production").is_err());
    }

    #[test]
    fn test_validate_private_key() {
        // Valid private key format (starts with S and 56 chars)
        assert!(InputValidator::validate_private_key("SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K").is_ok());

        // Invalid private keys
        assert!(InputValidator::validate_private_key("").is_err());
        assert!(InputValidator::validate_private_key("ABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K").is_err());
        assert!(InputValidator::validate_private_key("SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6").is_err());
    }

    #[test]
    fn test_validate_mnemonic() {
        // Valid mnemonic lengths
        assert!(InputValidator::validate_mnemonic("abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual").is_ok());
        assert!(InputValidator::validate_mnemonic("abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act").is_ok());

        // Invalid mnemonics
        assert!(InputValidator::validate_mnemonic("").is_err());
        assert!(InputValidator::validate_mnemonic("abandon ability able").is_err()); // Too short
        assert!(InputValidator::validate_mnemonic("abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual").is_err()); // Too long
    }

    #[test]
    fn test_validate_range() {
        // Valid ranges
        assert!(InputValidator::validate_range("50", 1.0, 100.0).is_ok());
        assert!(InputValidator::validate_range("1", 1.0, 100.0).is_ok());
        assert!(InputValidator::validate_range("100", 1.0, 100.0).is_ok());

        // Invalid ranges
        assert!(InputValidator::validate_range("0", 1.0, 100.0).is_err());
        assert!(InputValidator::validate_range("101", 1.0, 100.0).is_err());
        assert!(InputValidator::validate_range("abc", 1.0, 100.0).is_err());
    }

    #[test]
    fn test_validate_batch_size() {
        // Valid batch sizes
        assert!(InputValidator::validate_batch_size(1).is_ok());
        assert!(InputValidator::validate_batch_size(500).is_ok());
        assert!(InputValidator::validate_batch_size(1000).is_ok());

        // Invalid batch sizes
        assert!(InputValidator::validate_batch_size(0).is_err());
        assert!(InputValidator::validate_batch_size(1001).is_err());
    }

    #[test]
    fn test_validate_timeout() {
        // Valid timeouts
        assert!(InputValidator::validate_timeout(1).is_ok());
        assert!(InputValidator::validate_timeout(1800).is_ok());
        assert!(InputValidator::validate_timeout(3600).is_ok());

        // Invalid timeouts
        assert!(InputValidator::validate_timeout(0).is_err());
        assert!(InputValidator::validate_timeout(3601).is_err());
    }

    #[test]
    fn test_validation_error_formatting() {
        let error = ValidationError::InvalidAddress("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string());
        let formatted = ErrorHandler::format_validation_error(&error);
        assert!(formatted.contains("❌ Invalid address"));
        assert!(formatted.contains("Stellar addresses start with 'G'"));

        let suggestion = ErrorHandler::suggest_fix(&error);
        assert!(suggestion.contains("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K"));
    }
}
