//! Example implementations of Horizon Client usage
//!
//! This file contains various examples showing how to use the Horizon client
//! in different scenarios.

#![allow(dead_code, unused_imports)]

use std::time::Duration;

// Note: These are pseudo-code examples. Actual usage requires proper imports.

/// Example 1: Basic account information retrieval
pub async fn example_get_account_info(account_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    //
    // let client = HorizonClient::public()?;
    // let path = format!("/accounts/{}", account_id);
    // let account = client.get(&path).await?;
    // println!("Account info: {:?}", account);
    //
    // Ok(())
    Ok(())
}

/// Example 2: Fetch ledgers with pagination
pub async fn example_fetch_ledgers() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    //
    // let client = HorizonClient::public()?;
    // let ledgers = client.get("/ledgers?limit=100&order=desc").await?;
    // println!("Ledgers: {:?}", ledgers);
    //
    // Ok(())
    Ok(())
}

/// Example 3: Custom configuration with longer timeouts
pub async fn example_custom_timeout() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
    // use std::time::Duration;
    //
    // let config = HorizonClientConfig {
    //     server_url: "https://horizon.stellar.org".to_string(),
    //     timeout: Duration::from_secs(60),  // 60 second timeout
    //     enable_logging: true,
    //     ..Default::default()
    // };
    //
    // let client = HorizonClient::with_config(config)?;
    // let response = client.get("/").await?;
    // println!("Root: {:?}", response);
    //
    // Ok(())
    Ok(())
}

/// Example 4: Health checking
pub async fn example_health_check() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    // use stellaraid_tools::horizon_client::health::{HorizonHealthChecker, HealthCheckConfig, HealthStatus};
    //
    // let client = HorizonClient::public()?;
    // let checker = HorizonHealthChecker::new(HealthCheckConfig::default());
    //
    // let result = checker.check(&client).await?;
    //
    // match result.status {
    //     HealthStatus::Healthy => println!("✓ Horizon is healthy ({:?})", result.response_time_ms),
    //     HealthStatus::Degraded => println!("⚠ Horizon is degraded ({:?}ms)", result.response_time_ms),
    //     HealthStatus::Unhealthy => println!("✗ Horizon is down: {}", result.error.unwrap_or_default()),
    //     HealthStatus::Unknown => println!("? Status unknown"),
    // }
    //
    // Ok(())
    Ok(())
}

/// Example 5: Error handling with retryability
pub async fn example_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    // use stellaraid_tools::horizon_error::HorizonError;
    //
    // let client = HorizonClient::public()?;
    //
    // match client.get("/ledgers").await {
    //     Ok(response) => {
    //         println!("Success: {:?}", response);
    //     }
    //     Err(HorizonError::RateLimited { retry_after }) => {
    //         println!("Rate limited! Retry after: {:?}", retry_after);
    //         // Wait and retry...
    //         tokio::time::sleep(retry_after).await;
    //         let _ = client.get("/ledgers").await;
    //     }
    //     Err(HorizonError::Timeout { duration }) => {
    //         println!("Request timed out after {:?}", duration);
    //     }
    //     Err(e) if e.is_retryable() => {
    //         println!("Retryable error: {}", e);
    //         // Retry logic...
    //     }
    //     Err(e) => {
    //         println!("Non-retryable error: {}", e);
    //         return Err(Box::new(e));
    //     }
    // }
    //
    // Ok(())
    Ok(())
}

/// Example 6: Rate limiter statistics
pub async fn example_rate_limit_stats() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    //
    // let client = HorizonClient::public()?;
    // let stats = client.rate_limiter_stats();
    //
    // println!("Rate limit configuration:");
    // println!("  Requests per hour: {}", stats.config.requests_per_hour);
    // println!("  Requests per minute: {}", stats.config.requests_per_minute);
    // println!("  Requests per second: {}", stats.config.requests_per_second);
    // println!("  Time until ready: {:?}", stats.time_until_ready);
    // println!("  Ready for request: {}", stats.is_ready());
    //
    // Ok(())
    Ok(())
}

/// Example 7: Cache management
pub async fn example_cache_management() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
    // use std::time::Duration;
    //
    // let config = HorizonClientConfig {
    //     enable_cache: true,
    //     cache_ttl: Duration::from_secs(60),
    //     ..Default::default()
    // };
    //
    // let client = HorizonClient::with_config(config)?;
    //
    // // First request - fetches from API
    // let _ = client.get("/ledgers?limit=1").await?;
    //
    // // Second request - retrieves from cache
    // let _ = client.get("/ledgers?limit=1").await?;
    //
    // // View cache statistics
    // if let Some(stats) = client.cache_stats().await {
    //     println!("Cache stats:");
    //     println!("  Entries: {}", stats.entries);
    //     println!("  Hits: {}", stats.hits);
    //     println!("  Misses: {}", stats.misses);
    // }
    //
    // // Clear cache if needed
    // client.clear_cache().await?;
    //
    // Ok(())
    Ok(())
}

/// Example 8: Private Horizon with custom rate limits
pub async fn example_private_horizon() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    //
    // // Setup for private Horizon with 1000 requests/second limit
    // let client = HorizonClient::private(
    //     "https://my-horizon.example.com",
    //     1000.0
    // )?;
    //
    // // Now make requests - they'll respect the custom rate limit
    // let response = client.get("/").await?;
    // println!("Private Horizon root: {:?}", response);
    //
    // Ok(())
    Ok(())
}

/// Example 9: Aggressive retry configuration
pub async fn example_aggressive_retry() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
    // use stellaraid_tools::horizon_retry::{RetryConfig, RetryPolicy};
    // use std::time::Duration;
    //
    // let config = HorizonClientConfig {
    //     retry_config: RetryConfig::aggressive(),  // 5 attempts
    //     retry_policy: RetryPolicy::AllRetryable,
    //     ..Default::default()
    // };
    //
    // let client = HorizonClient::with_config(config)?;
    // // This request might fail network 4 times before succeeding or giving up
    // let response = client.get("/ledgers").await?;
    // println!("Got response after retries: {:?}", response);
    //
    // Ok(())
    Ok(())
}

/// Example 10: Continuous health monitoring
pub async fn example_health_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    // use stellaraid_tools::horizon_client::health::{HorizonHealthChecker, HealthMonitor};
    //
    // let client = HorizonClient::public()?;
    // let checker = HorizonHealthChecker::default_config();
    // let monitor = HealthMonitor::new(checker, 60); // Check every 60 seconds
    //
    // // Start background monitoring
    // monitor.start(client.clone()).await;
    //
    // // Do some work...
    // tokio::time::sleep(Duration::from_secs(10)).await;
    //
    // // Stop monitoring
    // monitor.stop();
    //
    // Ok(())
    Ok(())
}

/// Example 11: Structured error investigation
pub async fn example_detailed_error_info(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::HorizonClient;
    // use stellaraid_tools::horizon_error::HorizonError;
    //
    // let client = HorizonClient::public()?;
    //
    // match client.get(path).await {
    //     Ok(response) => {
    //         println!("Success: {:?}", response);
    //     }
    //     Err(error) => {
    //         println!("Error Classification:");
    //         println!("  Message: {}", error);
    //         println!("  Retryable: {}", error.is_retryable());
    //         println!("  Server error: {}", error.is_server_error());
    //         println!("  Client error: {}", error.is_client_error());
    //         println!("  Rate limited: {}", error.is_rate_limited());
    //
    //         if let Some(duration) = error.suggested_retry_duration() {
    //             println!("  Suggested retry after: {:?}", duration);
    //         }
    //
    //         return Err(Box::new(error));
    //     }
    // }
    //
    // Ok(())
    Ok(())
}

/// Example 12: Testing configuration
pub async fn example_test_setup() -> Result<(), Box<dyn std::error::Error>> {
    // use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
    //
    // // In tests, use a configuration without rate limiting or retries
    // let client = HorizonClient::with_config(HorizonClientConfig::test())?;
    //
    // // Now requests will be instant and won't retry
    // let response = client.get("/test-endpoint").await?;
    // println!("Test response: {:?}", response);
    //
    // Ok(())
    Ok(())
}
