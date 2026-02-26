//! Rate Limiting for Horizon API
//!
//! Implements rate limiting to respect Horizon API limits.
//! Horizon public has a limit of 72 requests per hour (1.2 requests per minute).

use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per second
    pub requests_per_second: f64,
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Maximum requests per hour
    pub requests_per_hour: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        // Horizon public limit: 72 requests per hour
        Self {
            requests_per_second: 0.02, // 1.2 per minute, or 72 per hour
            requests_per_minute: 1,    // Conservative: 1.2 per minute
            requests_per_hour: 72,     // Horizon public limit
        }
    }
}

impl RateLimitConfig {
    /// Create a rate limiter for a public Horizon instance
    /// (72 requests per hour limit)
    pub fn public_horizon() -> Self {
        Self::default()
    }

    /// Create a rate limiter for a private Horizon instance
    /// (typically higher limits)
    pub fn private_horizon(requests_per_second: f64) -> Self {
        Self {
            requests_per_second,
            requests_per_minute: (requests_per_second * 60.0) as u32,
            requests_per_hour: (requests_per_second * 3600.0) as u32,
        }
    }

    /// Create an unlimited rate limiter (for testing)
    pub fn unlimited() -> Self {
        Self {
            requests_per_second: 1000.0,
            requests_per_minute: 60000,
            requests_per_hour: 3600000,
        }
    }
}

/// Rate limiter for Horizon API requests
pub struct HorizonRateLimiter {
    /// Governor rate limiter
    limiter: Arc<RateLimiter>,
    /// Configuration
    config: RateLimitConfig,
}

impl HorizonRateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        // Convert requests per hour to a quota
        // Using non-zero value: requests per hour minimum is 1
        let quota = Quota::per_hour(NonZeroU32::new(config.requests_per_hour).unwrap_or(NonZeroU32::new(1).unwrap()));
        let limiter = RateLimiter::direct(quota);

        Self {
            limiter: Arc::new(limiter),
            config,
        }
    }

    /// Create a limiter for public Horizon (72 requests/hour)
    pub fn public_horizon() -> Self {
        Self::new(RateLimitConfig::public_horizon())
    }

    /// Create a limiter for private Horizon
    pub fn private_horizon(requests_per_second: f64) -> Self {
        Self::new(RateLimitConfig::private_horizon(requests_per_second))
    }

    /// Check if a request can be made immediately
    pub fn check(&self) -> bool {
        self.limiter.check().is_ok()
    }

    /// Wait until a request can be made
    pub async fn acquire(&self) {
        // Use a simple async wait approach
        while !self.check() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Try to acquire permission to make a request
    /// Returns the number of cells that were saturated if rate limit exceeded
    pub fn try_acquire(&self) -> Result<(), u32> {
        match self.limiter.check() {
            Ok(()) => Ok(()),
            Err(negative) => Err(negative.wait_time_from(std::time::Instant::now()).as_secs() as u32),
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Get estimated time until next request is allowed (in milliseconds)
    pub fn time_until_ready(&self) -> Option<Duration> {
        match self.limiter.check() {
            Ok(()) => Some(Duration::from_secs(0)),
            Err(negative) => Some(negative.wait_time_from(std::time::Instant::now())),
        }
    }

    /// Get statistics about rate limiter usage
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats {
            config: self.config.clone(),
            time_until_ready: self.time_until_ready(),
        }
    }
}

impl Clone for HorizonRateLimiter {
    fn clone(&self) -> Self {
        Self {
            limiter: Arc::clone(&self.limiter),
            config: self.config.clone(),
        }
    }
}

/// Statistics about rate limiter usage
#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    /// Rate limit configuration
    pub config: RateLimitConfig,
    /// Time until next request can be made
    pub time_until_ready: Option<Duration>,
}

impl RateLimiterStats {
    /// Check if rate limiter is ready for immediate request
    pub fn is_ready(&self) -> bool {
        self.time_until_ready == Some(Duration::from_secs(0))
    }

    /// Get estimated wait time in milliseconds
    pub fn wait_time_ms(&self) -> u64 {
        self.time_until_ready
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_hour, 72); // Horizon public limit
    }

    #[test]
    fn test_public_horizon_config() {
        let config = RateLimitConfig::public_horizon();
        assert_eq!(config.requests_per_hour, 72);
    }

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = HorizonRateLimiter::public_horizon();
        assert_eq!(limiter.config().requests_per_hour, 72);
    }

    #[test]
    fn test_unlimited_rate_limiter() {
        let limiter = HorizonRateLimiter::new(RateLimitConfig::unlimited());
        // Should allow immediate request
        assert!(limiter.check());
    }

    #[tokio::test]
    async fn test_acquire_async() {
        let limiter = HorizonRateLimiter::new(RateLimitConfig::unlimited());
        // Should acquire immediately without blocking
        limiter.acquire().await;
        assert!(limiter.check());
    }

    #[test]
    fn test_rate_limiter_stats() {
        let limiter = HorizonRateLimiter::public_horizon();
        let stats = limiter.stats();
        assert_eq!(stats.config.requests_per_hour, 72);
    }

    #[test]
    fn test_clone_rate_limiter() {
        let limiter1 = HorizonRateLimiter::public_horizon();
        let limiter2 = limiter1.clone();
        assert_eq!(limiter1.config().requests_per_hour, limiter2.config().requests_per_hour);
    }
}
