//! Asset Configuration
//!
//! Defines all supported Stellar assets with their metadata.

use soroban_sdk::{contracttype, String};

/// Represents a Stellar asset
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StellarAsset {
    /// Asset code (e.g., "XLM", "USDC")
    pub code: String,
    /// Issuer account address (empty for native XLM)
    pub issuer: String,
    /// Number of decimal places
    pub decimals: u32,
}

/// Asset information including metadata
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetInfo {
    pub asset: StellarAsset,
    /// Full name of the asset
    pub name: String,
    /// Organization/issuer name
    pub organization: String,
    /// Additional description
    pub description: String,
    /// Whether this is a native asset
    pub is_native: bool,
}

/// Asset metadata registry
pub struct AssetRegistry;

impl AssetRegistry {
    /// Native XLM asset
    pub fn xlm() -> StellarAsset {
        StellarAsset {
            code: String::from_slice(&soroban_sdk::Env::default(), "XLM"),
            issuer: String::from_slice(&soroban_sdk::Env::default(), ""),
            decimals: 7,
        }
    }

    /// USDC on Stellar ([Circle](https://www.circle.com/))
    pub fn usdc() -> StellarAsset {
        StellarAsset {
            code: String::from_slice(&soroban_sdk::Env::default(), "USDC"),
            issuer: String::from_slice(
                &soroban_sdk::Env::default(),
                "GA5ZSEJYB37JRC5AVCIA5MOP4GZ5DA47EL4PMRV4ZU5KHSUCZMVDXEN",
            ),
            decimals: 6,
        }
    }

    /// NGNT - Nigerian Naira Token
    pub fn ngnt() -> StellarAsset {
        StellarAsset {
            code: String::from_slice(&soroban_sdk::Env::default(), "NGNT"),
            issuer: String::from_slice(
                &soroban_sdk::Env::default(),
                "GAUYTZ24ATZTPC35NYSTSIHIVGZSC5THJOsimplicc4B3TDTFSLOMNLDA",
            ),
            decimals: 6,
        }
    }

    /// USDT (Tether) on Stellar
    pub fn usdt() -> StellarAsset {
        StellarAsset {
            code: String::from_slice(&soroban_sdk::Env::default(), "USDT"),
            issuer: String::from_slice(
                &soroban_sdk::Env::default(),
                "GBBD47UZQ2EOPIB6NYVTG2ND4VS4F7IJDLLUOYRCG76K7JT45XE7VAT",
            ),
            decimals: 6,
        }
    }

    /// EURT - Euro Token on Stellar
    pub fn eurt() -> StellarAsset {
        StellarAsset {
            code: String::from_slice(&soroban_sdk::Env::default(), "EURT"),
            issuer: String::from_slice(
                &soroban_sdk::Env::default(),
                "GAP5LETOV6YIE272RLUBZTV3QQF5JGKZ5FWXVMMP4QSXG7GSTF5GNBE7",
            ),
            decimals: 6,
        }
    }

    /// Returns all supported assets
    pub fn all_assets() -> [StellarAsset; 5] {
        [
            Self::xlm(),
            Self::usdc(),
            Self::ngnt(),
            Self::usdt(),
            Self::eurt(),
        ]
    }

    /// Returns all asset codes
    pub fn all_codes() -> [&'static str; 5] {
        ["XLM", "USDC", "NGNT", "USDT", "EURT"]
    }
}

impl StellarAsset {
    /// Check if this is the native XLM asset
    pub fn is_xlm(&self) -> bool {
        self.code.len() == 3
            && self
                .code
                .eq(&String::from_slice(&soroban_sdk::Env::default(), "XLM"))
            && self.issuer.is_empty()
    }

    /// Get the unique identifier for this asset
    pub fn id(&self) -> String {
        if self.is_xlm() {
            return String::from_slice(&soroban_sdk::Env::default(), "XLM");
        }
        // For non-native assets, combine code and issuer
        let env = soroban_sdk::Env::default();
        let mut id = self.code.clone();
        id.append(&self.issuer);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_xlm_asset() {
        let xlm = AssetRegistry::xlm();
        assert_eq!(xlm.code.len(), 3);
        assert_eq!(xlm.decimals, 7);
        assert!(xlm.is_xlm());
    }

    #[test]
    fn test_usdc_asset() {
        let usdc = AssetRegistry::usdc();
        assert_eq!(usdc.code.len(), 4);
        assert_eq!(usdc.decimals, 6);
        assert!(!usdc.is_xlm());
    }

    #[test]
    fn test_asset_codes() {
        let codes = AssetRegistry::all_codes();
        assert_eq!(codes.len(), 5);
        assert!(codes.contains(&"XLM"));
        assert!(codes.contains(&"USDC"));
        assert!(codes.contains(&"NGNT"));
        assert!(codes.contains(&"USDT"));
        assert!(codes.contains(&"EURT"));
    }
}
