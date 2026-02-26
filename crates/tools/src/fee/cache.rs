use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{FeeError, FeeResult};

/// Default cache TTL: 5 minutes
pub const DEFAULT_CACHE_TTL_SECS: i64 = 300;

/// Cached fee data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFeeData {
    /// Base fee in stroops
    pub base_fee_stroops: i64,
    /// When this data was fetched
    pub fetched_at: DateTime<Utc>,
    /// TTL in seconds
    pub ttl_seconds: i64,
}

impl CachedFeeData {
    /// Create new cached fee data
    pub fn new(base_fee_stroops: i64, ttl_seconds: i64) -> FeeResult<Self> {
        if base_fee_stroops < 0 {
            return Err(FeeError::InvalidFeeValue(
                "base_fee_stroops cannot be negative".to_string(),
            ));
        }

        Ok(Self {
            base_fee_stroops,
            fetched_at: Utc::now(),
            ttl_seconds,
        })
    }

    /// Check if cached data is still valid
    pub fn is_valid(&self) -> bool {
        let age_seconds = (Utc::now() - self.fetched_at).num_seconds();
        age_seconds < self.ttl_seconds
    }

    /// Get age of cached data in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.fetched_at).num_seconds()
    }

    /// Get time until expiration in seconds
    pub fn time_until_expiration(&self) -> i64 {
        (self.ttl_seconds - self.age_seconds()).max(0)
    }
}

/// Fee cache with 5 minute TTL
pub struct FeeCache {
    data: Option<CachedFeeData>,
    ttl_seconds: i64,
}

impl FeeCache {
    /// Create new fee cache
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            data: None,
            ttl_seconds,
        }
    }

    /// Create cache with default TTL (5 minutes)
    pub fn default_ttl() -> Self {
        Self::new(DEFAULT_CACHE_TTL_SECS)
    }

    /// Store base fee in cache
    pub fn set(&mut self, base_fee_stroops: i64) -> FeeResult<()> {
        self.data = Some(CachedFeeData::new(base_fee_stroops, self.ttl_seconds)?);
        Ok(())
    }

    /// Get base fee from cache if valid
    pub fn get(&self) -> Option<i64> {
        self.data.as_ref().and_then(|data| {
            if data.is_valid() {
                Some(data.base_fee_stroops)
            } else {
                None
            }
        })
    }

    /// Get base fee from cache regardless of validity
    pub fn get_unchecked(&self) -> Option<i64> {
        self.data.as_ref().map(|data| data.base_fee_stroops)
    }

    /// Check if cache has valid data
    pub fn is_valid(&self) -> bool {
        self.data.as_ref().map_or(false, |d| d.is_valid())
    }

    /// Check if cache has any data (valid or expired)
    pub fn has_data(&self) -> bool {
        self.data.is_some()
    }

    /// Clear cache
    pub fn clear(&mut self) {
        self.data = None;
    }

    /// Get cache metadata
    pub fn metadata(&self) -> Option<CacheMetadata> {
        self.data.as_ref().map(|data| CacheMetadata {
            base_fee_stroops: data.base_fee_stroops,
            fetched_at: data.fetched_at,
            age_seconds: data.age_seconds(),
            time_until_expiration: data.time_until_expiration(),
            is_valid: data.is_valid(),
        })
    }

    /// Set custom TTL
    pub fn set_ttl(&mut self, ttl_seconds: i64) {
        self.ttl_seconds = ttl_seconds;
    }
}

/// Cache metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub base_fee_stroops: i64,
    pub fetched_at: DateTime<Utc>,
    pub age_seconds: i64,
    pub time_until_expiration: i64,
    pub is_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_set_and_get() {
        let mut cache = FeeCache::default_ttl();
        assert!(cache.get().is_none());

        cache.set(100).unwrap();
        assert_eq!(cache.get(), Some(100));
    }

    #[test]
    fn test_cache_validity() {
        let mut cache = FeeCache::new(10); // 10 second TTL for testing
        cache.set(100).unwrap();

        assert!(cache.is_valid());
        assert!(cache.has_data());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = FeeCache::default_ttl();
        cache.set(100).unwrap();
        assert!(cache.has_data());

        cache.clear();
        assert!(!cache.has_data());
        assert!(cache.get().is_none());
    }

    #[test]
    fn test_cache_invalid_fee() {
        let mut cache = FeeCache::default_ttl();
        let result = cache.set(-100);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_metadata() {
        let mut cache = FeeCache::new(60);
        cache.set(100).unwrap();

        let metadata = cache.metadata().unwrap();
        assert_eq!(metadata.base_fee_stroops, 100);
        assert!(metadata.is_valid);
        assert!(metadata.age_seconds >= 0);
        assert!(metadata.time_until_expiration <= 60);
    }

    #[test]
    fn test_cache_get_unchecked() {
        let mut cache = FeeCache::new(1); // Very short TTL
        cache.set(100).unwrap();

        // Should return value even if expired
        assert_eq!(cache.get_unchecked(), Some(100));
    }

    #[test]
    fn test_cache_set_ttl() {
        let mut cache = FeeCache::new(10);
        assert_eq!(cache.ttl_seconds, 10);

        cache.set_ttl(20);
        assert_eq!(cache.ttl_seconds, 20);
    }

    #[test]
    fn test_cached_fee_data_creation() {
        let data = CachedFeeData::new(100, 300).unwrap();
        assert_eq!(data.base_fee_stroops, 100);
        assert_eq!(data.ttl_seconds, 300);
    }

    #[test]
    fn test_cached_fee_data_age() {
        let data = CachedFeeData::new(100, 300).unwrap();
        assert!(data.age_seconds() >= 0);
        assert!(data.is_valid());
    }
}
