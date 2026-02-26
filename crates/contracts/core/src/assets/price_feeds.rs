//! Asset Price Feed Integration
//!
//! Provides optional integration with price feed oracles for Stellar assets.
//! This module defines interfaces for price feed data and valuation.

use soroban_sdk::{contracttype, String};

/// Represents a price data point for an asset
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PriceData {
    /// Asset code
    pub asset_code: String,
    /// Price in USD (or base currency)
    pub price: i128,
    /// Number of decimal places for the price
    pub decimals: u32,
    /// Timestamp of the price (Unix epoch)
    pub timestamp: u64,
    /// Source of the price (e.g., "coingecko", "stellar-protocol/soroswap")
    pub source: String,
}

/// Represents conversion rates between assets
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ConversionRate {
    /// Source asset code
    pub from_asset: String,
    /// Target asset code
    pub to_asset: String,
    /// Conversion rate (how many `to_asset` units per 1 `from_asset`)
    pub rate: i128,
    /// Decimal places for the rate
    pub decimals: u32,
    /// Timestamp of the conversion rate
    pub timestamp: u64,
}

/// Configuration for price feed sources
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PriceFeedConfig {
    /// Primary oracle address
    pub oracle_address: String,
    /// Fallback oracle address
    pub fallback_oracle: String,
    /// Maximum age of price data (in seconds)
    pub max_price_age: u64,
    /// Whether to use oracle prices
    pub use_oracle: bool,
}

impl Default for PriceFeedConfig {
    fn default() -> Self {
        let env = soroban_sdk::Env::default();
        Self {
            oracle_address: String::from_slice(&env, ""),
            fallback_oracle: String::from_slice(&env, ""),
            max_price_age: 3600, // 1 hour
            use_oracle: false,
        }
    }
}

/// Price feed provider interface
///
/// This interface defines how to interact with price feed sources.
/// Implementation would depend on specific oracle integration (e.g., Soroswap, Stellar Protocol oracles)
pub struct PriceFeedProvider;

impl PriceFeedProvider {
    /// Get price data for an asset
    ///
    /// In a real implementation, this would query an oracle
    pub fn get_price(_asset_code: &str) -> Option<PriceData> {
        // Placeholder implementation
        // Real implementation would fetch from oracle
        None
    }

    /// Get conversion rate between two assets
    ///
    /// In a real implementation, this would calculate rate from price data
    pub fn get_conversion_rate(from: &str, to: &str) -> Option<ConversionRate> {
        // Placeholder implementation
        // Real implementation would fetch from oracle or calculate from prices
        None
    }

    /// Convert an amount from one asset to another
    pub fn convert(from_asset: &str, to_asset: &str, amount: i128) -> Option<i128> {
        if let Some(rate) = Self::get_conversion_rate(from_asset, to_asset) {
            // Apply conversion: amount * rate / 10^decimals
            Some((amount * rate) / (10_i128.pow(rate.decimals)))
        } else {
            None
        }
    }

    /// Check if price data is fresh
    pub fn is_price_fresh(price: &PriceData, max_age: u64, current_time: u64) -> bool {
        current_time.saturating_sub(price.timestamp) < max_age
    }

    /// Validate price data
    pub fn validate_price(price: &PriceData) -> bool {
        // Check that price is positive
        price.price > 0 && price.decimals <= 18
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_rate_default() {
        let env = soroban_sdk::Env::default();
        let rate = ConversionRate {
            from_asset: String::from_slice(&env, "XLM"),
            to_asset: String::from_slice(&env, "USDC"),
            rate: 2_500_000, // 0.25 USDC per XLM (6 decimals)
            decimals: 6,
            timestamp: 1000,
        };
        assert!(rate.rate > 0);
    }

    #[test]
    fn test_validate_price() {
        let env = soroban_sdk::Env::default();
        let valid_price = PriceData {
            asset_code: String::from_slice(&env, "XLM"),
            price: 12_345_000, // $0.12345
            decimals: 6,
            timestamp: 1000,
            source: String::from_slice(&env, "coingecko"),
        };
        assert!(PriceFeedProvider::validate_price(&valid_price));

        let invalid_price = PriceData {
            asset_code: String::from_slice(&env, "XLM"),
            price: -1, // Invalid negative price
            decimals: 6,
            timestamp: 1000,
            source: String::from_slice(&env, "coingecko"),
        };
        assert!(!PriceFeedProvider::validate_price(&invalid_price));
    }

    #[test]
    fn test_is_price_fresh() {
        let env = soroban_sdk::Env::default();
        let price = PriceData {
            asset_code: String::from_slice(&env, "XLM"),
            price: 12_345_000,
            decimals: 6,
            timestamp: 1000,
            source: String::from_slice(&env, "coingecko"),
        };

        // Price from 1000 seconds ago, max age 3600 seconds
        assert!(PriceFeedProvider::is_price_fresh(&price, 3600, 2000));

        // Price too old
        assert!(!PriceFeedProvider::is_price_fresh(&price, 500, 2000));
    }

    #[test]
    fn test_price_feed_config_default() {
        let config = PriceFeedConfig::default();
        assert_eq!(config.max_price_age, 3600);
        assert!(!config.use_oracle);
    }
}
