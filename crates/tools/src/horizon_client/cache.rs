//! Response Caching for Horizon API
//!
//! Implements optional response caching to reduce API calls.

use crate::horizon_error::HorizonResult;
use moka::future::Cache;
use serde_json::Value;
use std::time::Duration;

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cache entries
    pub entries: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
}

/// Response cache for Horizon API
pub struct ResponseCache {
    /// Internal cache
    cache: Cache<String, Value>,
    /// Hit count
    hits: std::sync::atomic::AtomicU64,
    /// Miss count
    misses: std::sync::atomic::AtomicU64,
}

impl ResponseCache {
    /// Create a new response cache with TTL
    pub fn new(ttl: Duration) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .build();

        Self {
            cache,
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get a cached response
    pub async fn get(&self, key: &str) -> HorizonResult<Value> {
        if let Some(value) = self.cache.get(key).await {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(value);
        }

        self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(crate::horizon_error::HorizonError::CacheError(
            "Cache miss".to_string(),
        ))
    }

    /// Set a cached response
    pub async fn set(&self, key: &str, value: Value) {
        self.cache.insert(key.to_string(), value).await;
    }

    /// Clear the cache
    pub async fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Get cache statistics
    pub fn stats(&self) -> Option<CacheStats> {
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed);

        Some(CacheStats {
            entries: self.cache.entry_count(),
            hits,
            misses,
        })
    }

    /// Get hit rate percentage
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let total = hits + misses;

        if total == 0.0 {
            0.0
        } else {
            (hits / total) * 100.0
        }
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        self.hits.store(0, std::sync::atomic::Ordering::Relaxed);
        self.misses.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let value = serde_json::json!({"test": "value"});

        cache.set("key1", value.clone()).await;
        let result = cache.get("key1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let result = cache.get("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let value = serde_json::json!({"test": "value"});

        cache.set("key1", value).await;
        cache.clear().await;

        let result = cache.get("key1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let value = serde_json::json!({"test": "value"});

        cache.set("key1", value.clone()).await;
        let _ = cache.get("key1").await; // Hit
        let _ = cache.get("key2").await; // Miss

        let stats = cache.stats();
        assert!(stats.is_some());
    }

    #[tokio::test]
    async fn test_cache_hit_rate() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let value = serde_json::json!({"test": "value"});

        // Populate cache
        cache.set("key1", value.clone()).await;

        // 2 hits, 1 miss
        let _ = cache.get("key1").await; // Hit
        let _ = cache.get("key1").await; // Hit
        let _ = cache.get("key2").await; // Miss

        let rate = cache.hit_rate();
        assert!(rate > 60.0 && rate < 70.0); // 2/3 ≈ 66.67%
    }
}
