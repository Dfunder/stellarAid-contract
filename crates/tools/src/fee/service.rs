use chrono::Utc;
use log::{info, warn, debug};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{
    cache::{FeeCache, DEFAULT_CACHE_TTL_SECS},
    calculator::{FeeInfo, FeeConfig},
    currency::{Currency, CurrencyConverter},
    error::FeeResult,
    history::FeeHistory,
    horizon_fetcher::HorizonFeeFetcher,
    surge_pricing::{SurgePricingAnalyzer, SurgePricingConfig},
};

/// Fee estimation service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeServiceConfig {
    /// Horizon server URL
    pub horizon_url: String,
    /// Cache TTL in seconds
    pub cache_ttl_secs: i64,
    /// Fetch timeout in seconds
    pub fetch_timeout_secs: u64,
    /// Max history records to keep
    pub max_history_records: usize,
    /// Enable fee spike detection
    pub enable_surge_detection: bool,
}

impl Default for FeeServiceConfig {
    fn default() -> Self {
        Self {
            horizon_url: "https://horizon.stellar.org".to_string(),
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
            fetch_timeout_secs: 30,
            max_history_records: 1000,
            enable_surge_detection: true,
        }
    }
}

/// Fee estimation service
pub struct FeeEstimationService {
    config: FeeServiceConfig,
    fee_config: FeeConfig,
    horizon_fetcher: HorizonFeeFetcher,
    cache: Arc<RwLock<FeeCache>>,
    history: Arc<RwLock<FeeHistory>>,
    surge_analyzer: Arc<RwLock<SurgePricingAnalyzer>>,
    converter: Arc<RwLock<CurrencyConverter>>,
}

impl FeeEstimationService {
    /// Create new fee estimation service
    pub fn new(config: FeeServiceConfig) -> Self {
        let horizon_fetcher = HorizonFeeFetcher::new(config.horizon_url.clone())
            .with_timeout(config.fetch_timeout_secs);

        let surge_config = SurgePricingConfig::default();
        let surge_analyzer = SurgePricingAnalyzer::new(surge_config);

        Self {
            config,
            fee_config: FeeConfig::default(),
            horizon_fetcher,
            cache: Arc::new(RwLock::new(FeeCache::new(
                config.cache_ttl_secs,
            ))),
            history: Arc::new(RwLock::new(FeeHistory::new(
                config.max_history_records,
            ))),
            surge_analyzer: Arc::new(RwLock::new(surge_analyzer)),
            converter: Arc::new(RwLock::new(CurrencyConverter::new())),
        }
    }

    /// Create service with default public Horizon
    pub fn public_horizon() -> Self {
        Self::new(FeeServiceConfig::default())
    }

    /// Estimate fee for operations
    pub async fn estimate_fee(&self, operation_count: u32) -> FeeResult<FeeInfo> {
        info!("Estimating fee for {} operations", operation_count);

        // Try to get fresh base fee from cache
        if let Some(cached_fee) = self.get_cached_fee().await {
            info!("Using cached base fee: {} stroops", cached_fee);
            return FeeInfo::new(
                cached_fee,
                operation_count,
                false,
                100.0,
            );
        }

        // Fetch fresh base fee from Horizon
        let base_fee = self.fetch_and_cache_fee().await?;

        // Check for surge pricing
        let surge_analyzer = self.surge_analyzer.write().await;
        let analysis = surge_analyzer.analyze(base_fee)?;

        info!("Base fee: {} stroops, Surge level: {}", base_fee, analysis.surge_level.name());

        let fee_info = FeeInfo::new(
            base_fee,
            operation_count,
            analysis.is_surge,
            analysis.surge_percent,
        )?;

        Ok(fee_info)
    }

    /// Estimate fee and convert to currency
    pub async fn estimate_fee_in_currency(
        &self,
        operation_count: u32,
        currency: Currency,
    ) -> FeeResult<(FeeInfo, f64)> {
        let fee_info = self.estimate_fee(operation_count).await?;

        if currency == Currency::XLM {
            return Ok((fee_info.clone(), fee_info.total_fee_xlm));
        }

        let converter = self.converter.read().await;
        let converted_amount = converter.convert_xlm_fee(fee_info.total_fee_xlm, currency)?;

        Ok((fee_info, converted_amount))
    }

    /// Set exchange rate for currency conversion
    pub async fn set_exchange_rate(
        &self,
        from: Currency,
        to: Currency,
        rate: f64,
    ) -> FeeResult<()> {
        let mut converter = self.converter.write().await;
        converter.set_rate(from, to, rate)?;
        Ok(())
    }

    /// Get fee history statistics
    pub async fn get_fee_stats(&self) -> Option<super::history::FeeStats> {
        let history = self.history.read().await;
        history.stats()
    }

    /// Get recent fee statistics
    pub async fn get_recent_fee_stats(&self, seconds: i64) -> Option<super::history::FeeStats> {
        let history = self.history.read().await;
        history.recent_stats(seconds)
    }

    /// Get cached fee without checking validity
    async fn get_cached_fee(&self) -> Option<i64> {
        let cache = self.cache.read().await;
        cache.get()
    }

    /// Fetch fee from Horizon and cache it
    async fn fetch_and_cache_fee(&self) -> FeeResult<i64> {
        debug!("Fetching base fee from Horizon");

        let base_fee = self.horizon_fetcher.fetch_base_fee().await?;

        // Store in cache
        let mut cache = self.cache.write().await;
        cache.set(base_fee)?;

        // Store in history
        let mut history = self.history.write().await;
        history.add(base_fee, "Horizon API".to_string())?;

        Ok(base_fee)
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("Fee cache cleared");
    }

    /// Clear history
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
        info!("Fee history cleared");
    }

    /// Get cache metadata
    pub async fn get_cache_metadata(&self) -> Option<super::cache::CacheMetadata> {
        let cache = self.cache.read().await;
        cache.metadata()
    }

    /// Get history records count
    pub async fn get_history_count(&self) -> usize {
        let history = self.history.read().await;
        history.len()
    }

    /// Batch estimate fees for multiple operations
    pub async fn batch_estimate_fees(&self, operation_counts: &[u32]) -> FeeResult<Vec<FeeInfo>> {
        let mut fees = Vec::new();

        for &count in operation_counts {
            let fee = self.estimate_fee(count).await?;
            fees.push(fee);
        }

        Ok(fees)
    }

    /// Detect if fees are currently surging
    pub async fn is_surging(&self) -> FeeResult<bool> {
        let fee_info = self.estimate_fee(1).await?;
        Ok(fee_info.is_surge_pricing)
    }

    /// Get surge pricing information
    pub async fn get_surge_info(&self) -> Option<String> {
        let base_fee = self.get_cached_fee().await?;
        let analyzer = self.surge_analyzer.read().await;
        let analysis = analyzer.analyze(base_fee).ok()?;

        Some(format!(
            "{}: {} ({}%)",
            analysis.surge_level.name(),
            analysis.recommendation,
            analysis.surge_percent as i64
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_service_config_default() {
        let config = FeeServiceConfig::default();
        assert_eq!(config.horizon_url, "https://horizon.stellar.org");
        assert_eq!(config.cache_ttl_secs, 300);
        assert!(!config.horizon_url.is_empty());
    }

    #[test]
    fn test_fee_service_creation() {
        let service = FeeEstimationService::public_horizon();
        assert!(!service.config.horizon_url.is_empty());
    }

    #[tokio::test]
    async fn test_fee_service_clear_cache() {
        let service = FeeEstimationService::public_horizon();
        service.clear_cache().await;
        // Should not panic
    }

    #[tokio::test]
    async fn test_fee_service_clear_history() {
        let service = FeeEstimationService::public_horizon();
        service.clear_history().await;

        let count = service.get_history_count().await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_fee_service_history_count() {
        let service = FeeEstimationService::public_horizon();
        let count = service.get_history_count().await;
        assert_eq!(count, 0);
    }

    #[test]
    fn test_fee_service_public_horizon() {
        let service = FeeEstimationService::public_horizon();
        assert_eq!(service.config.horizon_url, "https://horizon.stellar.org");
    }
}
