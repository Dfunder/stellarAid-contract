//! Horizon API Client
//!
//! A robust client for interacting with Stellar Horizon API with:
//! - Error handling for network and API errors
//! - Rate limiting to respect Horizon limits
//! - Retry logic with exponential backoff
//! - Request logging and health checks

pub mod cache;
pub mod health;

use crate::horizon_error::{HorizonError, HorizonResult};
use crate::horizon_rate_limit::{HorizonRateLimiter, RateLimitConfig};
use crate::horizon_retry::{calculate_backoff, RetryConfig, RetryPolicy};
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use reqwest::{Client, ClientBuilder, StatusCode, Timeout};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Configuration for Horizon client
#[derive(Debug, Clone)]
pub struct HorizonClientConfig {
    /// Base URL for Horizon API (e.g., https://horizon.stellar.org)
    pub server_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Enable request logging
    pub enable_logging: bool,
    /// Rate limit configuration
    pub rate_limit_config: RateLimitConfig,
    /// Retry configuration
    pub retry_config: RetryConfig,
    /// Retry policy
    pub retry_policy: RetryPolicy,
    /// Enable response caching
    pub enable_cache: bool,
    /// Cache TTL
    pub cache_ttl: Duration,
}

impl Default for HorizonClientConfig {
    fn default() -> Self {
        Self {
            server_url: "https://horizon.stellar.org".to_string(),
            timeout: Duration::from_secs(30),
            enable_logging: cfg!(debug_assertions),
            rate_limit_config: RateLimitConfig::public_horizon(),
            retry_config: RetryConfig::default(),
            retry_policy: RetryPolicy::default(),
            enable_cache: true,
            cache_ttl: Duration::from_secs(60),
        }
    }
}

impl HorizonClientConfig {
    /// Create configuration for public Horizon (with rate limiting)
    pub fn public_horizon() -> Self {
        Self::default()
    }

    /// Create configuration for private Horizon instance
    pub fn private_horizon(url: impl Into<String>, requests_per_second: f64) -> Self {
        Self {
            server_url: url.into(),
            rate_limit_config: RateLimitConfig::private_horizon(requests_per_second),
            ..Default::default()
        }
    }

    /// Create configuration for testing (no rate limiting, no retries)
    pub fn test() -> Self {
        Self {
            server_url: "http://localhost:8000".to_string(),
            timeout: Duration::from_secs(5),
            enable_logging: false,
            rate_limit_config: RateLimitConfig::unlimited(),
            retry_config: RetryConfig::none(),
            retry_policy: RetryPolicy::NoRetry,
            enable_cache: false,
            cache_ttl: Duration::from_secs(0),
        }
    }
}

/// Request context for tracking and logging
#[derive(Debug, Clone)]
struct RequestContext {
    /// Unique request ID
    request_id: String,
    /// Request start time
    start_time: DateTime<Utc>,
    /// Attempt number
    attempt: u32,
}

impl RequestContext {
    /// Create a new request context
    fn new() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            start_time: Utc::now(),
            attempt: 1,
        }
    }

    /// Get elapsed duration since request started
    fn elapsed(&self) -> Duration {
        (Utc::now() - self.start_time)
            .to_std()
            .unwrap_or(Duration::from_secs(0))
    }
}

/// Horizon API Client
#[derive(Clone)]
pub struct HorizonClient {
    /// Configuration
    config: HorizonClientConfig,
    /// HTTP client
    http_client: Arc<Client>,
    /// Rate limiter
    rate_limiter: HorizonRateLimiter,
    /// Response cache (optional)
    cache: Option<Arc<cache::ResponseCache>>,
}

impl HorizonClient {
    /// Create a new Horizon client with default configuration
    pub fn new() -> HorizonResult<Self> {
        Self::with_config(HorizonClientConfig::default())
    }

    /// Create a new Horizon client with custom configuration
    pub fn with_config(config: HorizonClientConfig) -> HorizonResult<Self> {
        let http_client = ClientBuilder::new()
            .timeout(Timeout::from_secs(config.timeout.as_secs()))
            .user_agent("stellaraid-client/1.0")
            .build()
            .map_err(|e| HorizonError::InvalidConfig(e.to_string()))?;

        let rate_limiter = HorizonRateLimiter::new(config.rate_limit_config.clone());

        let cache = if config.enable_cache {
            Some(Arc::new(cache::ResponseCache::new(config.cache_ttl)))
        } else {
            None
        };

        info!("Horizon client initialized for {}", config.server_url);

        Ok(Self {
            config,
            http_client: Arc::new(http_client),
            rate_limiter,
            cache,
        })
    }

    /// Create a client for public Horizon
    pub fn public() -> HorizonResult<Self> {
        Self::with_config(HorizonClientConfig::public_horizon())
    }

    /// Create a client for private Horizon
    pub fn private(url: impl Into<String>, requests_per_second: f64) -> HorizonResult<Self> {
        Self::with_config(HorizonClientConfig::private_horizon(url, requests_per_second))
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.config.server_url
    }

    /// Get configuration
    pub fn config(&self) -> &HorizonClientConfig {
        &self.config
    }

    /// Get rate limiter stats
    pub fn rate_limiter_stats(&self) -> crate::horizon_rate_limit::RateLimiterStats {
        self.rate_limiter.stats()
    }

    /// Make a GET request to Horizon
    pub async fn get(&self, path: &str) -> HorizonResult<serde_json::Value> {
        // Check cache first if enabled
        if let Some(cache) = &self.cache {
            if let Ok(cached) = cache.get(path).await {
                debug!("Cache hit for {}", path);
                return Ok(cached);
            }
        }

        let url = format!("{}{}", self.config.server_url, path);
        let context = RequestContext::new();

        self.execute_with_retry(&context, || {
            Box::pin({
                let url = url.clone();
                let http_client = Arc::clone(&self.http_client);
                let context = context.clone();

                async move {
                    // Respect rate limits
                    self.rate_limiter.acquire().await;

                    if self.config.enable_logging {
                        debug!(
                            "[{}] GET {} (attempt {})",
                            context.request_id, url, context.attempt
                        );
                    }

                    let response = http_client
                        .get(&url)
                        .send()
                        .await
                        .map_err(|e| HorizonError::from_reqwest(e))?;

                    let status = response.status();

                    // Handle rate limiting headers
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(Duration::from_secs)
                            .unwrap_or(Duration::from_secs(60));

                        return Err(HorizonError::RateLimited {
                            retry_after,
                        });
                    }

                    if !status.is_success() {
                        let body = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        return match status {
                            StatusCode::NOT_FOUND => Err(HorizonError::NotFound(body)),
                            StatusCode::BAD_REQUEST => Err(HorizonError::BadRequest(body)),
                            StatusCode::UNAUTHORIZED => Err(HorizonError::Unauthorized(body)),
                            StatusCode::FORBIDDEN => Err(HorizonError::Forbidden(body)),
                            s if s.is_server_error() => Err(HorizonError::ServerError {
                                status: s.as_u16(),
                                message: body,
                            }),
                            s => Err(HorizonError::HttpError {
                                status: s.as_u16(),
                                message: body,
                            }),
                        };
                    }

                    let json = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|e| HorizonError::InvalidResponse(e.to_string()))?;

                    if self.config.enable_logging {
                        debug!(
                            "[{}] GET {} completed in {:?}",
                            context.request_id,
                            url,
                            context.elapsed()
                        );
                    }

                    Ok(json)
                }
            })
        })
        .await?;

        // Cache the response if enabled
        if let Some(cache) = &self.cache {
            let _ = cache.set(path, result.clone()).await;
        }

        Ok(result)
    }

    /// Execute with retry logic
    async fn execute_with_retry<F, T>(
        &self,
        context: &RequestContext,
        mut f: F,
    ) -> HorizonResult<T>
    where
        F: FnMut() -> futures::future::BoxFuture<'static, HorizonResult<T>>,
    {
        let mut errors = Vec::new();

        for attempt in 1..=self.config.retry_config.max_attempts {
            let mut ctx = context.clone();
            ctx.attempt = attempt;

            match f().await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    errors.push(error.clone());

                    // Check retry policy
                    if !self.config.retry_policy.should_retry(&error) {
                        error!(
                            "[{}] Request failed with non-retryable error: {}",
                            ctx.request_id, error
                        );
                        return Err(error);
                    }

                    // Check if we have more attempts
                    if attempt >= self.config.retry_config.max_attempts {
                        error!(
                            "[{}] Request failed after {} attempts: {}",
                            ctx.request_id, attempt, error
                        );
                        return Err(error);
                    }

                    // Calculate backoff
                    let backoff =
                        calculate_backoff(attempt, &self.config.retry_config);

                    warn!(
                        "[{}] Request failed on attempt {}/{}, retrying after {:?}: {}",
                        ctx.request_id, attempt, self.config.retry_config.max_attempts, backoff, error
                    );

                    tokio::time::sleep(backoff).await;
                }
            }
        }

        Err(errors.pop().unwrap_or_else(|| {
            HorizonError::Other("Unknown retry error".to_string())
        }))
    }

    /// Clear the response cache
    pub async fn clear_cache(&self) -> HorizonResult<()> {
        if let Some(cache) = &self.cache {
            cache.clear().await;
            info!("Response cache cleared");
        }
        Ok(())
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> Option<cache::CacheStats> {
        self.cache.as_ref().and_then(|c| c.stats())
    }
}

impl Default for HorizonClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default HorizonClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_defaults() {
        let config = HorizonClientConfig::default();
        assert_eq!(config.timeout.as_secs(), 30);
    }

    #[test]
    fn test_client_config_public() {
        let config = HorizonClientConfig::public_horizon();
        assert!(config.server_url.contains("horizon.stellar.org"));
    }

    #[test]
    fn test_client_config_test() {
        let config = HorizonClientConfig::test();
        assert_eq!(config.timeout.as_secs(), 5);
        assert!(!config.enable_logging);
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = HorizonClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_request_context() {
        let ctx = RequestContext::new();
        assert!(!ctx.request_id.is_empty());
        assert!(ctx.elapsed() < Duration::from_secs(1));
    }
}
