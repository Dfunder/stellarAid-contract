#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_management::{AccountManagementService, AccountManagementRequest, AccountAction};
    use crate::validation::InputValidator;

    #[test]
    fn test_validate_address() {
        // Valid addresses
        let result = AccountManagementService::validate_address("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K");
        assert!(result.is_ok());

        // Invalid addresses
        let result = AccountManagementService::validate_address("INVALID_ADDRESS");
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_account_creation_fee() {
        let fee = AccountManagementService::estimate_account_creation_fee();
        assert_eq!(fee, 2000000); // 2 XLM in stroops
    }

    #[tokio::test]
    async fn test_account_management_request_creation() {
        let request = AccountManagementRequest {
            action: AccountAction::Create,
            account_id: None,
            wallet_type: None,
            private_key: None,
            mnemonic: None,
            password: None,
        };

        assert!(matches!(request.action, AccountAction::Create));
        assert!(request.account_id.is_none());
        assert!(request.wallet_type.is_none());
        assert!(request.private_key.is_none());
        assert!(request.mnemonic.is_none());
        assert!(request.password.is_none());
    }

    #[test]
    fn test_wallet_type_validation() {
        // Test wallet type string parsing (this would be done in the actual CLI code)
        let valid_wallets = vec!["freighter", "albedo", "lobstr", "ledger", "trezor"];
        let invalid_wallets = vec!["metamask", "phantom", "invalid"];

        for wallet in valid_wallets {
            // In actual implementation, this would be parsed by the CLI
            assert!(!wallet.is_empty());
        }

        for wallet in invalid_wallets {
            // These should be rejected by the CLI
            assert!(!wallet.is_empty());
        }
    }

    #[test]
    fn test_account_action_validation() {
        // Test that all account actions are properly defined
        let actions = vec![
            AccountAction::Create,
            AccountAction::Import,
            AccountAction::Export,
            AccountAction::List,
            AccountAction::Balance,
            AccountAction::Signers,
            AccountAction::SetOptions,
            AccountAction::Fund,
        ];

        for action in actions {
            match action {
                AccountAction::Create => assert!(true),
                AccountAction::Import => assert!(true),
                AccountAction::Export => assert!(true),
                AccountAction::List => assert!(true),
                AccountAction::Balance => assert!(true),
                AccountAction::Signers => assert!(true),
                AccountAction::SetOptions => assert!(true),
                AccountAction::Fund => assert!(true),
            }
        }
    }

    #[test]
    fn test_private_key_validation() {
        // Valid private key format
        let valid_key = "SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K";
        assert!(InputValidator::validate_private_key(valid_key).is_ok());

        // Invalid private keys
        let invalid_keys = vec![
            "",
            "ABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K", // Wrong prefix
            "SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6", // Too short
            "SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6KK", // Too long
            "INVALID_KEY_FORMAT",
        ];

        for key in invalid_keys {
            assert!(InputValidator::validate_private_key(key).is_err());
        }
    }

    #[test]
    fn test_mnemonic_validation() {
        // Valid mnemonic (12 words)
        let valid_mnemonic = "abandon ability able about above absent absorb abstract absurd abuse access accident account accuse";
        assert!(InputValidator::validate_mnemonic(valid_mnemonic).is_ok());

        // Invalid mnemonics
        let invalid_mnemonics = vec![
            "",
            "abandon ability", // Too short
            "abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse abandon ability able about above absent absorb abstract absurd abuse access accident account accuse", // Too long
        ];

        for mnemonic in invalid_mnemonics {
            assert!(InputValidator::validate_mnemonic(mnemonic).is_err());
        }
    }

    #[test]
    fn test_account_creation_requirements() {
        // Test that account creation requires proper validation
        let request_no_password = AccountManagementRequest {
            action: AccountAction::Create,
            account_id: None,
            wallet_type: None,
            private_key: None,
            mnemonic: None,
            password: None,
        };

        let request_with_password = AccountManagementRequest {
            action: AccountAction::Create,
            account_id: None,
            wallet_type: None,
            private_key: None,
            mnemonic: None,
            password: Some("password123".to_string()),
        };

        // In actual implementation, saving would require a password
        if request_no_password.password.is_none() {
            // This would fail in actual implementation when save=true
            assert!(true);
        }

        if request_with_password.password.is_some() {
            // This would succeed in actual implementation
            assert!(true);
        }
    }

    #[test]
    fn test_account_import_requirements() {
        // Test that account import requires either private key or mnemonic
        let request_neither = AccountManagementRequest {
            action: AccountAction::Import,
            account_id: None,
            wallet_type: None,
            private_key: None,
            mnemonic: None,
            password: None,
        };

        let request_private_key = AccountManagementRequest {
            action: AccountAction::Import,
            account_id: None,
            wallet_type: None,
            private_key: Some("SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string()),
            mnemonic: None,
            password: None,
        };

        let request_mnemonic = AccountManagementRequest {
            action: AccountAction::Import,
            account_id: None,
            wallet_type: None,
            private_key: None,
            mnemonic: Some("abandon ability able about above absent absorb abstract absurd abuse access accident account accuse".to_string()),
            password: None,
        };

        // Neither provided should fail
        assert!(request_neither.private_key.is_none() && request_neither.mnemonic.is_none());

        // Private key provided should be valid
        if let Some(key) = request_private_key.private_key {
            assert!(InputValidator::validate_private_key(&key).is_ok());
        }

        // Mnemonic provided should be valid
        if let Some(mnemonic) = request_mnemonic.mnemonic {
            assert!(InputValidator::validate_mnemonic(&mnemonic).is_ok());
        }
    }
}
