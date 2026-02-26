//! Asset Validation Utilities
//!
//! Provides validation logic for assets and trust lines.

use soroban_sdk::String;

use super::config::StellarAsset;
use super::resolver::AssetResolver;

/// Errors that can occur during asset validation
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetValidationError {
    /// Asset is not supported
    UnsupportedAsset,
    /// Asset code is invalid
    InvalidAssetCode,
    /// Asset issuer is invalid
    InvalidIssuer,
    /// Asset has incorrect decimals
    IncorrectDecimals,
    /// Trust line not established
    TrustLineNotEstablished,
    /// Insufficient trust line balance
    InsufficientTrustLineBalance,
    /// Asset metadata mismatch
    MetadataMismatch,
}

/// Asset validator for checking asset validity and trust lines
pub struct AssetValidator;

impl AssetValidator {
    /// Validate that an asset is supported
    pub fn validate_asset(asset: &StellarAsset) -> Result<(), AssetValidationError> {
        if !AssetResolver::validate(asset) {
            return Err(AssetValidationError::UnsupportedAsset);
        }
        Ok(())
    }

    /// Check if an asset code is valid (3-12 character alphanumeric)
    pub fn is_valid_asset_code(code: &str) -> bool {
        if code.is_empty() || code.len() > 12 {
            return false;
        }

        code.chars().all(|c| c.is_ascii_alphanumeric())
    }

    /// Check if an issuer address seems valid (basic check)
    /// Note: Full validation would require address validation utilities
    pub fn is_valid_issuer(issuer: &str) -> bool {
        if issuer.is_empty() {
            // Empty issuer is valid for native XLM
            return true;
        }

        // Basic check: should be 56 characters and start with 'G'
        issuer.len() == 56 && issuer.starts_with('G')
    }

    /// Verify asset has correct decimals for supported assets
    pub fn verify_decimals(asset: &StellarAsset) -> Result<(), AssetValidationError> {
        match asset.code.as_raw().as_slice() {
            b"XLM" => {
                if asset.decimals == 7 {
                    Ok(())
                } else {
                    Err(AssetValidationError::IncorrectDecimals)
                }
            }
            b"USDC" | b"NGNT" | b"USDT" | b"EURT" => {
                if asset.decimals == 6 {
                    Ok(())
                } else {
                    Err(AssetValidationError::IncorrectDecimals)
                }
            }
            _ => Err(AssetValidationError::InvalidAssetCode),
        }
    }

    /// Validate complete asset structure
    pub fn validate_complete(asset: &StellarAsset) -> Result<(), AssetValidationError> {
        // Check asset code validity
        let code_str: &str = std::str::from_utf8(asset.code.as_raw().as_slice())
            .map_err(|_| AssetValidationError::InvalidAssetCode)?;

        if !Self::is_valid_asset_code(code_str) {
            return Err(AssetValidationError::InvalidAssetCode);
        }

        // Check issuer validity
        let issuer_str: &str = std::str::from_utf8(asset.issuer.as_raw().as_slice())
            .map_err(|_| AssetValidationError::InvalidIssuer)?;

        if !Self::is_valid_issuer(issuer_str) {
            return Err(AssetValidationError::InvalidIssuer);
        }

        // Check decimals
        Self::verify_decimals(asset)?;

        // Check if asset is supported
        Self::validate_asset(asset)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::config::AssetRegistry;

    #[test]
    fn test_valid_asset_code() {
        assert!(AssetValidator::is_valid_asset_code("XLM"));
        assert!(AssetValidator::is_valid_asset_code("USDC"));
        assert!(AssetValidator::is_valid_asset_code("ABCDEF1234"));
        assert!(!AssetValidator::is_valid_asset_code(""));
        assert!(!AssetValidator::is_valid_asset_code(&"A".repeat(13)));
    }

    #[test]
    fn test_valid_issuer() {
        // Valid issuer
        assert!(AssetValidator::is_valid_issuer(
            "GA5ZSEJYB37JRC5AVCIA5MOP4GZ5DA47EL4PMRV4ZU5KHSUCZMVDXEN"
        ));
        // Empty issuer (native asset)
        assert!(AssetValidator::is_valid_issuer(""));
        // Invalid issuer
        assert!(!AssetValidator::is_valid_issuer("INVALID"));
    }

    #[test]
    fn test_verify_decimals() {
        let xlm = AssetRegistry::xlm();
        assert!(AssetValidator::verify_decimals(&xlm).is_ok());

        let usdc = AssetRegistry::usdc();
        assert!(AssetValidator::verify_decimals(&usdc).is_ok());
    }

    #[test]
    fn test_validate_asset() {
        let xlm = AssetRegistry::xlm();
        assert!(AssetValidator::validate_asset(&xlm).is_ok());

        let usdc = AssetRegistry::usdc();
        assert!(AssetValidator::validate_asset(&usdc).is_ok());
    }
}
