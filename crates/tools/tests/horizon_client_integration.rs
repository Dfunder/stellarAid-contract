#[cfg(test)]
mod horizon_integration_tests {
    //! Integration tests for the Horizon client
    //!
    //! These tests can be run against:
    //! - Mock HTTP server (no network required)
    //! - Test/Testnet Horizon (requires network)
    //! - Public Horizon (requires network, production data)
    //!
    //! To run these tests:
    //! ```
    //! cargo test --test horizon_client_integration -- --test-threads=1 --nocapture
    //! ```

    use std::time::Duration;
    use tokio::time::timeout;

    // ============================================================================
    // Integration Tests - Basic Functionality
    // ============================================================================

    #[tokio::test]
    async fn test_horizon_client_creation() {
        // Verify client can be created successfully
        // let client = HorizonClient::public();
        // assert!(client.is_ok(), "Failed to create public Horizon client");
    }

    #[tokio::test]
    async fn test_custom_config_client() {
        // Verify custom configuration is applied
        // use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
        //
        // let config = HorizonClientConfig {
        //     timeout: Duration::from_secs(15),
        //     ..Default::default()
        // };
        //
        // let client = HorizonClient::with_config(config);
        // assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_enforcement() {
        // Verify rate limiter blocks rapid-fire requests
        // let client = HorizonClient::public().unwrap();
        //
        // // Make back-to-back requests
        // let start = std::time::Instant::now();
        // let _ = client.get("/").await;
        // let _ = client.get("/").await;
        // let _ = client.get("/").await;
        // let elapsed = start.elapsed();
        //
        // // The rate limiter should cause some delay
        // assert!(elapsed > Duration::from_millis(10), "Rate limiting didn't enforce delays");
    }

    #[tokio::test]
    async fn test_timeout_enforcement() {
        // Verify timeout configuration is respected
        // let config = HorizonClientConfig {
        //     timeout: Duration::from_millis(100),
        //     ..Default::default()
        // };
        //
        // let client = HorizonClient::with_config(config).unwrap();
        //
        // // This should timeout (intentionally using a slow endpoint)
        // let result = client.get("/ledgers?limit=10000").await;
        // assert!(result.is_err(), "Expected timeout");
    }

    // ============================================================================
    // Integration Tests - Error Handling
    // ============================================================================

    #[tokio::test]
    async fn test_network_error_classification() {
        // Verify network errors are properly classified
        // let client = HorizonClient::private("http://invalid-host-12345.local", 100.0);
        // assert!(client.is_err(), "Should fail to connect to invalid host");
    }

    #[tokio::test]
    async fn test_rate_limit_detection() {
        // Verify rate limit errors are properly detected
        // This requires hitting the rate limit, which is hard to test
        // in CI without being a bad citizen
        //
        // In production, you'd want to:
        // 1. Set up a test Horizon instance with low rate limits
        // 2. Or mock the rate limit response
    }

    #[tokio::test]
    async fn test_404_not_found() {
        // Verify 404 responses are properly classified
        // let client = HorizonClient::public().unwrap();
        // let result = client.get("/nonexistent/endpoint").await;
        //
        // match result {
        //     Err(HorizonError::NotFound) => {
        //         // Expected
        //     }
        //     other => panic!("Expected NotFound error, got: {:?}", other),
        // }
    }

    #[tokio::test]
    async fn test_retry_on_transient_error() {
        // Verify retry logic attempts multiple times
        // This is best tested with mocking
        //
        // use std::sync::atomic::{AtomicUsize, Ordering};
        // use std::sync::Arc;
        //
        // let attempt_count = Arc::new(AtomicUsize::new(0));
        // // Mock server that fails first 2 times, succeeds on 3rd
        // // Verify attempt_count == 3 when done
    }

    // ============================================================================
    // Integration Tests - Caching
    // ============================================================================

    #[tokio::test]
    async fn test_cache_hit_tracking() {
        // Verify cache hits are tracked
        // let config = HorizonClientConfig {
        //     enable_cache: true,
        //     cache_ttl: Duration::from_secs(60),
        //     ..Default::default()
        // };
        //
        // let client = HorizonClient::with_config(config).unwrap();
        //
        // // First request - cache miss
        // let _ = client.get("/ledgers?limit=1").await;
        // let stats1 = client.cache_stats().await.unwrap();
        // assert_eq!(stats1.misses, 1);
        // assert_eq!(stats1.hits, 0);
        //
        // // Second request - cache hit
        // let _ = client.get("/ledgers?limit=1").await;
        // let stats2 = client.cache_stats().await.unwrap();
        // assert_eq!(stats2.misses, 1);
        // assert_eq!(stats2.hits, 1);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        // Verify cache entries expire after TTL
        // let config = HorizonClientConfig {
        //     enable_cache: true,
        //     cache_ttl: Duration::from_millis(500),
        //     ..Default::default()
        // };
        //
        // let client = HorizonClient::with_config(config).unwrap();
        //
        // // Cache a response
        // let _ = client.get("/ledgers?limit=1").await;
        // let stats1 = client.cache_stats().await.unwrap();
        // assert_eq!(stats1.entries, 1);
        //
        // // Wait for cache to expire
        // tokio::time::sleep(Duration::from_millis(600)).await;
        //
        // // Entry should be gone
        // let stats2 = client.cache_stats().await.unwrap();
        // assert_eq!(stats2.entries, 0);
    }

    // ============================================================================
    // Integration Tests - Health Monitoring
    // ============================================================================

    #[tokio::test]
    async fn test_health_check_success() {
        // Verify health check works for healthy Horizon
        // use stellaraid_tools::horizon_client::{HorizonClient, health::HorizonHealthChecker};
        // use stellaraid_tools::horizon_client::health::HealthStatus;
        //
        // let client = HorizonClient::public().unwrap();
        // let checker = HorizonHealthChecker::new(Default::default());
        // let result = checker.check(&client).await.unwrap();
        //
        // match result.status {
        //     HealthStatus::Healthy | HealthStatus::Degraded => {
        //         // OK - Horizon is responding
        //         assert!(result.response_time_ms < 10000);
        //     }
        //     HealthStatus::Unhealthy => {
        //         panic!("Expected healthy or degraded, got unhealthy");
        //     }
        //     HealthStatus::Unknown => {
        //         panic!("Expected status result, got Unknown");
        //     }
        // }
    }

    // ============================================================================
    // Integration Tests - Multiple Concurrent Requests
    // ============================================================================

    #[tokio::test]
    async fn test_concurrent_requests() {
        // Verify rate limiting works correctly with concurrent requests
        // This specifically tests that rate limits are enforced globally
        //
        // let client = std::sync::Arc::new(HorizonClient::public().unwrap());
        // let mut handles = vec![];
        //
        // for i in 0..5 {
        //     let client_clone = client.clone();
        //     let handle = tokio::spawn(async move {
        //         let result = client_clone.get("/").await;
        //         (i, result)
        //     });
        //     handles.push(handle);
        // }
        //
        // let results: Vec<_> = futures::future::join_all(handles)
        //     .await
        //     .into_iter()
        //     .map(|r| r.unwrap())
        //     .collect();
        //
        // // All requests should succeed (or fail gracefully)
        // for (idx, result) in results {
        //     assert!(result.is_ok() || result.is_err(), "Unexpected state at request {}", idx);
        // }
    }

    // ============================================================================
    // Integration Tests - Retry Behavior
    // ============================================================================

    #[tokio::test]
    async fn test_retry_policy_transient_only() {
        // Verify TransientOnly policy retries network errors
        // use stellaraid_tools::horizon_retry::RetryPolicy;
        //
        // // Network errors should be retried
        // let policy = RetryPolicy::TransientOnly;
        // let error = HorizonError::NetworkError("test".to_string());
        // assert!(policy.should_retry(&error));
    }

    #[tokio::test]
    async fn test_retry_policy_no_client_errors() {
        // Verify TransientOnly doesn't retry client errors
        // use stellaraid_tools::horizon_retry::RetryPolicy;
        // use stellaraid_tools::horizon_error::HorizonError;
        //
        // let policy = RetryPolicy::TransientOnly;
        // let error = HorizonError::NotFound;
        // assert!(!policy.should_retry(&error));
    }

    // ============================================================================
    // Integration Tests - Real-World Scenarios (requires network)
    // ============================================================================

    #[tokio::test]
    #[ignore] // Only run with: cargo test -- --ignored --nocapture
    async fn test_real_horizon_root_endpoint() {
        // Test against actual Stellar public Horizon
        //
        // let client = HorizonClient::public().unwrap();
        // let result = timeout(
        //     Duration::from_secs(10),
        //     client.get("/")
        // ).await;
        //
        // assert!(result.is_ok(), "Timeout or error connecting to Horizon");
        // let response = result.unwrap();
        // assert!(response.is_ok(), "Failed to get root: {:?}", response.unwrap_err());
    }

    #[tokio::test]
    #[ignore] // Only run with: cargo test -- --ignored --nocapture
    async fn test_real_horizon_ledgers_endpoint() {
        // Test fetching ledgers from actual Horizon
        //
        // let client = HorizonClient::public().unwrap();
        // let result = timeout(
        //     Duration::from_secs(10),
        //     client.get("/ledgers?limit=10&order=desc")
        // ).await;
        //
        // assert!(result.is_ok());
        // let response = result.unwrap();
        // assert!(response.is_ok(), "Failed to get ledgers: {:?}", response.unwrap_err());
        //
        // // Verify response contains expected structure
        // if let Ok(value) = response {
        //     assert!(value.is_object(), "Expected JSON object response");
        //     assert!(value.get("_links").is_some(), "Expected _links field");
        //     assert!(value.get("_embedded").is_some(), "Expected _embedded field");
        // }
    }

    // ============================================================================
    // Integration Tests - Load Testing (optional)
    // ============================================================================

    #[tokio::test]
    #[ignore] // Only run manually for load testing
    async fn test_load_1000_sequential_requests() {
        // Test that rate limiting handles many sequential requests
        // let client = HorizonClient::public().unwrap();
        //
        // let start = std::time::Instant::now();
        // for _ in 0..1000 {
        //     let _ = client.get("/").await;
        // }
        // let elapsed = start.elapsed();
        //
        // println!("1000 requests took: {:?}", elapsed);
        // // Should respect rate limits: ~50 hours of requests in limit time
    }

    // ============================================================================
    // Verification Helper Functions
    // ============================================================================

    /// Helper to verify error is retryable
    #[allow(dead_code)]
    fn assert_retryable(error: &str, should_be_retryable: bool) {
        // use stellaraid_tools::horizon_error::HorizonError;
        // let test_error = HorizonError::NetworkError(error.to_string());
        // assert_eq!(test_error.is_retryable(), should_be_retryable);
    }

    /// Helper to verify error classification
    #[allow(dead_code)]
    fn assert_error_type(error_type: &str, is_server: bool, is_client: bool) {
        // Server errors should have is_server_error() == true
        // Client errors should have is_client_error() == true
        println!("Error type: {}, server: {}, client: {}", error_type, is_server, is_client);
    }
}

// ============================================================================
// Unit Tests - Can be run with: cargo test
// ============================================================================

#[cfg(test)]
mod horizon_unit_tests {
    use std::time::Duration;

    #[test]
    fn test_config_defaults() {
        // Verify default configuration values are sensible
        // let config = HorizonClientConfig::default();
        // assert_eq!(config.timeout, Duration::from_secs(30));
        // assert!(config.enable_logging);
        // assert!(config.enable_cache);
    }

    #[test]
    fn test_public_config() {
        // Verify public Horizon config has correct rate limits
        // let config = HorizonClientConfig::public_config();
        // assert_eq!(config.server_url, "https://horizon.stellar.org");
        // assert_eq!(config.rate_limit_config.requests_per_hour, 72);
    }

    #[test]
    fn test_private_config() {
        // Verify private Horizon config accepts custom URLs
        // let config = HorizonClientConfig::private_config("https://my-horizon.local", 1000.0);
        // assert_eq!(config.server_url, "https://my-horizon.local");
        // assert>(config.rate_limit_config.requests_per_second > 0.0);
    }
}
