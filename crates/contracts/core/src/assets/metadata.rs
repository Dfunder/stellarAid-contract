//! Asset Metadata
//!
//! Provides metadata about supported assets including names, descriptions, and visual assets.

use soroban_sdk::{contracttype, String};

/// Asset visual metadata (icons, logos, etc.)
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetVisuals {
    /// URL to asset icon (e.g., 32x32 PNG)
    pub icon_url: String,
    /// URL to asset logo (high resolution)
    pub logo_url: String,
    /// Brand color in hex format
    pub color: String,
}

/// Complete asset metadata
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetMetadata {
    /// Asset code
    pub code: String,
    /// Full name of the asset
    pub name: String,
    /// Issuing organization
    pub organization: String,
    /// Asset description
    pub description: String,
    /// Visual assets (icons and logos)
    pub visuals: AssetVisuals,
    /// Website URL
    pub website: String,
}

/// Asset metadata registry
pub struct MetadataRegistry;

impl MetadataRegistry {
    /// Get metadata for XLM
    pub fn xlm() -> AssetMetadata {
        let env = soroban_sdk::Env::default();
        AssetMetadata {
            code: String::from_slice(&env, "XLM"),
            name: String::from_slice(&env, "Stellar Lumens"),
            organization: String::from_slice(&env, "Stellar Development Foundation"),
            description: String::from_slice(
                &env,
                "The native asset of the Stellar network, used for transaction fees and network operations",
            ),
            visuals: AssetVisuals {
                icon_url: String::from_slice(
                    &env,
                    "https://assets.coingecko.com/coins/images/new_logos/stellar-lumens-xlm-logo.svg",
                ),
                logo_url: String::from_slice(
                    &env,
                    "https://assets.coingecko.com/coins/images/stellar-lumens-xlm-logo.png",
                ),
                color: String::from_slice(&env, "#14B8A6"),
            },
            website: String::from_slice(&env, "https://stellar.org"),
        }
    }

    /// Get metadata for USDC
    pub fn usdc() -> AssetMetadata {
        let env = soroban_sdk::Env::default();
        AssetMetadata {
            code: String::from_slice(&env, "USDC"),
            name: String::from_slice(&env, "USD Coin"),
            organization: String::from_slice(&env, "Circle"),
            description: String::from_slice(
                &env,
                "The leading alternative to USDT. USDC is the bridge between dollars and crypto.",
            ),
            visuals: AssetVisuals {
                icon_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GA5ZSEJYB37JRC5AVCIA5MOP4GZ5DA47EL4PMRV4ZU5KHSUCZMVDXEN/logo.png",
                ),
                logo_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GA5ZSEJYB37JRC5AVCIA5MOP4GZ5DA47EL4PMRV4ZU5KHSUCZMVDXEN/logo.png",
                ),
                color: String::from_slice(&env, "#2775CA"),
            },
            website: String::from_slice(&env, "https://www.circle.com/usdc"),
        }
    }

    /// Get metadata for NGNT
    pub fn ngnt() -> AssetMetadata {
        let env = soroban_sdk::Env::default();
        AssetMetadata {
            code: String::from_slice(&env, "NGNT"),
            name: String::from_slice(&env, "Nigerian Naira Token"),
            organization: String::from_slice(&env, "Stellar Foundation"),
            description: String::from_slice(
                &env,
                "A stablecoin representing Nigerian Naira, enabling local currency transactions on Stellar",
            ),
            visuals: AssetVisuals {
                icon_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GAUYTZ24ATZTPC35NYSTSIHIVGZSC5THJOsimplicc4B3TDTFSLOMNLDA/logo.png",
                ),
                logo_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GAUYTZ24ATZTPC35NYSTSIHIVGZSC5THJOsimplicc4B3TDTFSLOMNLDA/logo.png",
                ),
                color: String::from_slice(&env, "#009E73"),
            },
            website: String::from_slice(&env, "https://stellar.org"),
        }
    }

    /// Get metadata for USDT
    pub fn usdt() -> AssetMetadata {
        let env = soroban_sdk::Env::default();
        AssetMetadata {
            code: String::from_slice(&env, "USDT"),
            name: String::from_slice(&env, "Tether"),
            organization: String::from_slice(&env, "Tether Limited"),
            description: String::from_slice(
                &env,
                "The original stablecoin, representing US Dollar on blockchain networks",
            ),
            visuals: AssetVisuals {
                icon_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GBBD47UZQ2EOPIB6NYVTG2ND4VS4F7IJDLLUOYRCG76K7JT45XE7VAT/logo.png",
                ),
                logo_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GBBD47UZQ2EOPIB6NYVTG2ND4VS4F7IJDLLUOYRCG76K7JT45XE7VAT/logo.png",
                ),
                color: String::from_slice(&env, "#26A17B"),
            },
            website: String::from_slice(&env, "https://tether.to"),
        }
    }

    /// Get metadata for EURT
    pub fn eurt() -> AssetMetadata {
        let env = soroban_sdk::Env::default();
        AssetMetadata {
            code: String::from_slice(&env, "EURT"),
            name: String::from_slice(&env, "Euro Token"),
            organization: String::from_slice(&env, "Wirex"),
            description: String::from_slice(
                &env,
                "A stablecoin backed by euros, enabling EUR transactions on Stellar",
            ),
            visuals: AssetVisuals {
                icon_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GAP5LETOV6YIE272RLUBZTV3QQF5JGKZ5FWXVMMP4QSXG7GSTF5GNBE7/logo.png",
                ),
                logo_url: String::from_slice(
                    &env,
                    "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/stellar/assets/GAP5LETOV6YIE272RLUBZTV3QQF5JGKZ5FWXVMMP4QSXG7GSTF5GNBE7/logo.png",
                ),
                color: String::from_slice(&env, "#003399"),
            },
            website: String::from_slice(&env, "https://wirex.com"),
        }
    }

    /// Get metadata by asset code
    pub fn get_by_code(code: &str) -> Option<AssetMetadata> {
        match code {
            "XLM" => Some(Self::xlm()),
            "USDC" => Some(Self::usdc()),
            "NGNT" => Some(Self::ngnt()),
            "USDT" => Some(Self::usdt()),
            "EURT" => Some(Self::eurt()),
            _ => None,
        }
    }

    /// Get all metadata entries
    pub fn all() -> [AssetMetadata; 5] {
        [
            Self::xlm(),
            Self::usdc(),
            Self::ngnt(),
            Self::usdt(),
            Self::eurt(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xlm_metadata() {
        let metadata = MetadataRegistry::xlm();
        assert_eq!(metadata.code.len(), 3);
        assert!(!metadata.organization.is_empty());
        assert!(!metadata.visuals.icon_url.is_empty());
    }

    #[test]
    fn test_get_metadata_by_code() {
        let usdc = MetadataRegistry::get_by_code("USDC");
        assert!(usdc.is_some());

        let invalid = MetadataRegistry::get_by_code("INVALID");
        assert!(invalid.is_none());
    }
}
