# Stellar Horizon API Client

A robust, production-ready Rust client for interacting with Stellar Horizon API with comprehensive error handling, rate limiting, retry logic, and monitoring.

## Features

### ✅ Robust Error Handling
- Comprehensive error types for all failure scenarios
- Network errors, timeouts, rate limiting, server errors
- Client error handling (4xx) and server error handling (5xx)
- Retryable vs non-retryable error classification
- Suggested retry durations based on error type

### ✅ Rate Limiting
- Respects Horizon public API limit (72 requests/hour)
- Support for private Horizon instances with custom limits
- Token bucket algorithm for fair rate limiting
- Async-friendly rate limiter with acquisition methods
- Rate limiter statistics and monitoring

### ✅ Retry Logic
- Exponential backoff with configurable parameters
- Jitter support to prevent thundering herd
- Configurable retry policies (transient-only, server errors, all retryable)
- Attempt tracking and context preservation
- Per-error retry duration suggestions

### ✅ Request Management
- Configurable request timeouts
- Request ID tracking for debugging
- Request logging with attempt numbers
- Response time tracking
- Elapsed time logging

### ✅ Response Caching (Optional)
- Async-compatible cache implementation
- Configurable TTL per request
- Cache statistics (hit rate, hit count, miss count)
- Hit/miss tracking for analytics
- Manual cache invalidation

### ✅ Health Monitoring
- Periodic health checks
- Health status tracking (Healthy, Degraded, Unhealthy)
- Response time thresholds
- Cached health results with configurable TTL
- Continuous monitoring background task

### ✅ Logging & Debugging
- Request/response logging in development
- Unique request IDs for tracking
- Attempt-level logging
- Error context logging
- Health check logs

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
stellaraid-tools = { path = "crates/tools" }
```

## Quick Start

### Basic Usage

```rust
use stellaraid_tools::horizon_client::HorizonClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Create a client for public Horizon
    let client = HorizonClient::public()?;

    // Make a request
    let ledgers = client.get("/ledgers?limit=10").await?;
    println!("Ledgers: {:?}", ledgers);

    Ok(())
}
```

### Custom Configuration

```rust
use stellaraid_tools::horizon_client::{HorizonClient, HorizonClientConfig};
use std::time::Duration;

let config = HorizonClientConfig {
    server_url: "https://horizon.stellar.org".to_string(),
    timeout: Duration::from_secs(30),
    enable_logging: true,
    ..Default::default()
};

let client = HorizonClient::with_config(config)?;
```

### Health Checks

```rust
use stellaraid_tools::horizon_client::health::{HorizonHealthChecker, HealthCheckConfig};

let checker = HorizonHealthChecker::new(HealthCheckConfig::default());
let client = HorizonClient::public()?;

let result = checker.check(&client).await?;
println!("Horizon status: {}", result.status);
println!("Response time: {}ms", result.response_time_ms);
```

### Rate Limiting Info

```rust
let client = HorizonClient::public()?;
let stats = client.rate_limiter_stats();

println!("Rate limit config: {:?}", stats.config);
println!("Time until ready: {:?}", stats.time_until_ready);
```

## Architecture

### Core Components

1. **HorizonClient** - Main client for API interactions
   - Configuration management
   - Request execution with retry
   - Cache management
   - Health checking integration

2. **HorizonError** - Comprehensive error types
   - Network errors
   - HTTP errors (4xx, 5xx)
   - Rate limiting
   - Timeouts
   - Retryability classification

3. **HorizonRateLimiter** - Rate limiting with token bucket
   - Governor-based implementation
   - Public Horizon limit: 72 requests/hour
   - Private Horizon custom limits
   - Statistics and monitoring

4. **RetryConfig & RetryPolicy** - Retry management
   - Exponential backoff calculation
   - Configurable retry strategies
   - Transient failure detection
   - Server error handling

5. **ResponseCache** - Optional caching layer
   - Moka async cache
   - TTL-based expiration
   - Statistics tracking
   - Hit rate monitoring

6. **HorizonHealthChecker** - Health monitoring
   - Status classification
   - Response time tracking
   - Cached results
   - Continuous monitoring

## Configuration

### Basic Configuration

```rust
// Public Horizon with default settings
let client = HorizonClient::public()?;

// Private Horizon with custom rate limiting
let client = HorizonClient::private(
    "https://my-horizon.example.com",
    100.0  // 100 requests per second
)?;

// Testing configuration
let client = HorizonClient::with_config(
    HorizonClientConfig::test()
)?;
```

### Advanced Configuration

```rust
use stellaraid_tools::horizon_client::{
    HorizonClientConfig, HorizonClient,
};
use stellaraid_tools::horizon_rate_limit::RateLimitConfig;
use stellaraid_tools::horizon_retry::{RetryConfig, RetryPolicy};
use std::time::Duration;

let config = HorizonClientConfig {
    server_url: "https://horizon.stellar.org".to_string(),
    timeout: Duration::from_secs(30),
    enable_logging: true,
    rate_limit_config: RateLimitConfig::public_horizon(),
    retry_config: RetryConfig {
        max_attempts: 5,           // Up to 5 retries
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_secs(60),
        backoff_multiplier: 2.0,  // Exponential
        use_jitter: true,          // Add randomness
    },
    retry_policy: RetryPolicy::TransientAndServerErrors,
    enable_cache: true,
    cache_ttl: Duration::from_secs(60),
};

let client = HorizonClient::with_config(config)?;
```

## Error Handling

### Checking Error Type

```rust
use stellaraid_tools::horizon_error::HorizonError;

match client.get("/some/path").await {
    Ok(response) => println!("Success: {:?}", response),
    Err(HorizonError::RateLimited { retry_after }) => {
        println!("Rate limited, retry after: {:?}", retry_after);
    }
    Err(HorizonError::NetworkError(msg)) => {
        println!("Network error: {}", msg);
    }
    Err(HorizonError::NotFound(msg)) => {
        println!("Resource not found: {}", msg);
    }
    Err(e) => println!("Error: {}", e),
}
```

### Retryability

```rust
let error = client.get("/path").await.unwrap_err();

if error.is_retryable() {
    println!("Error is retryable");
}

if error.is_server_error() {
    println!("Server error detected");
}

if let Some(duration) = error.suggested_retry_duration() {
    println!("Suggested retry after: {:?}", duration);
}
```

## Rate Limiting

### Understanding Limits

- **Public Horizon**: 72 requests per hour (approximately 1.2 per minute)
- **Private Horizon**: Configurable based on your server

### Rate Limit Statistics

```rust
let client = HorizonClient::public()?;
let stats = client.rate_limiter_stats();

println!("Configured limit: {}/hour", stats.config.requests_per_hour);
println!("Time until next request: {:?}", stats.time_until_ready);
println!("Ready? {}", stats.is_ready());
```

### Handling Rate Limits

The client automatically respects rate limits through the `acquire()` method:

```rust
// The client waits until rate limit allows the request
let response = client.get("/path").await?;
```

### Custom Rate Limiting

```rust
use stellaraid_tools::horizon_rate_limit::{HorizonRateLimiter, RateLimitConfig};

// Create a private Horizon limiter (1000 requests/second)
let limiter = HorizonRateLimiter::private_horizon(1000.0);

// Check if request is allowed
if limiter.check() {
    // Make request immediately
}

// Or wait for permission
limiter.acquire().await;
// Now safe to make request
```

## Caching

### Enable/Disable Caching

```rust
let mut config = HorizonClientConfig::public_horizon();
config.enable_cache = true;
config.cache_ttl = Duration::from_secs(60);

let client = HorizonClient::with_config(config)?;
```

### Cache Management

```rust
// The client automatically caches GET responses

// Get cache statistics
if let Some(stats) = client.cache_stats().await {
    println!("Cache entries: {}", stats.entries);
    println!("Cache hits: {}", stats.hits);
    println!("Cache misses: {}", stats.misses);
}

// Clear cache manually
client.clear_cache().await?;
```

## Health Monitoring

### Periodic Health Checks

```rust
use stellaraid_tools::horizon_client::health::{
    HorizonHealthChecker, HealthCheckConfig, HealthStatus,
};

let checker = HorizonHealthChecker::new(HealthCheckConfig {
    timeout_ms: 5000,
    cache_duration_ms: 30000,
    degraded_threshold_ms: 2000,
});

let result = checker.check(&client).await?;

match result.status {
    HealthStatus::Healthy => println!("Horizon is healthy"),
    HealthStatus::Degraded => println!("Horizon is slow ({}ms)", result.response_time_ms),
    HealthStatus::Unhealthy => println!("Horizon is down"),
    HealthStatus::Unknown => println!("Status unknown"),
}
```

###Continuous Monitoring

```rust
use stellaraid_tools::horizon_client::health::HealthMonitor;

let checker = HorizonHealthChecker::default_config();
let monitor = HealthMonitor::new(checker, 60); // Check every 60 seconds

monitor.start(client.clone()).await;

// Later...
monitor.stop();
```

## Retry Strategies

### Transient Failures Only

```rust
use stellaraid_tools::horizon_retry::RetryPolicy;

let config = HorizonClientConfig {
    retry_policy: RetryPolicy::TransientOnly,
    ..Default::default()
};
```

Retries on:
- Network errors
- Timeouts
- Connection issues
- DNS errors

### Transient + Server Errors

```rust
let config = HorizonClientConfig {
    retry_policy: RetryPolicy::TransientAndServerErrors,
    ..Default::default()
};
```

Also retries on:
- 5xx server errors
- Service unavailable

### All Retryable Errors

```rust
let config = HorizonClientConfig {
    retry_policy: RetryPolicy::AllRetryable,
    ..Default::default()
};
```

Retries on all errors classified as retryable.

### No Retry

```rust
let config = HorizonClientConfig {
    retry_policy: RetryPolicy::NoRetry,
    ..Default::default()
};
```

## Logging

### Enable Logging

Logging is enabled by default in debug builds. To enable in release:

```rust
let config = HorizonClientConfig {
    enable_logging: true,
    ..Default::default()
};

let client = HorizonClient::with_config(config)?;
```

### Example Output

```
[DEBUG] [550e8400-e29b-41d4-a716-446655440000] GET https://horizon.stellar.org/ledgers (attempt 1)
[DEBUG] [550e8400-e29b-41d4-a716-446655440000] GET https://horizon.stellar.org/ledgers completed in 145ms
[INFO] Horizon client initialized for https://horizon.stellar.org
[WARN] [550e8400-e29b-41d4-a716-446655440001] Request failed on attempt 1/3, retrying after 100ms: Network error: connection reset
```

## Best Practices

### 1. Use Connection Pooling
The client uses `reqwest::Client` internally which handles connection pooling automatically.

### 2. Respect Rate Limits
Always use the public/private Horizon configuration appropriate for your use case.

### 3. Implement Backoff
Use the retry configuration to implement exponential backoff:

```rust
let config = HorizonClientConfig {
    retry_config: RetryConfig::aggressive(), // Up to 5 retries
    ..Default::default()
};
```

### 4. Monitor Health
Implement periodic health checks to detect Horizon issues early:

```rust
let checker = HorizonHealthChecker::default_config();
let monitor = HealthMonitor::new(checker, 300); // Check every 5 minutes
monitor.start(client.clone()).await;
```

### 5. Handle Errors Appropriately

```rust
match client.get("/path").await {
    Ok(data) => process_data(data),
    Err(e) if e.is_retryable() => {
        // Could retry manually if needed
        log::warn!("Retryable error: {}", e);
    }
    Err(e) => {
        // Non-retryable error
        log::error!("Fatal error: {}", e);
        return Err(e);
    }
}
```

## Testing

### Test Configuration

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client() {
        let client = HorizonClient::with_config(
            HorizonClientConfig::test()
        ).unwrap();

        // Your tests here
    }
}
```

Test configuration includes:
- No rate limiting
- No retries
- No caching
- Disabledlogging
- Local server URL

## Troubleshooting

### Rate Limited Errors
**Problem**: Getting 429 Too Many Requests
**Solution**: 
1. Increase request spacing
2. Implement caching for repeated queries
3. Consider using private Horizon for production

### Timeout Errors
**Problem**: Requests timing out
**Solution**:
1. Increase timeout configuration
2. Check network connectivity
3. Monitor Horizon uptime

### Network Errors
**Problem**: Connection refused or network unreachable
**Solution**:
1. Verify Horizon URL is correct
2. Check firewall rules
3. Implement retry logic

## Performance

- **Rate Limiter**: O(1) with atomic operations
- **Cache**: O(1) average case (moka hash map)
- **Retry Logic**: O(n) where n = max attempts (typically 3-5)
- **Health Check**: Single HTTP request (~200-500ms)

## Dependencies

- `reqwest` - HTTP client
- `tokio` - Async runtime
- `governor` - Rate limiting
- `moka` - Async caching
- `chrono` - Timestamp handling
- `log` - Logging facade
- `thiserror` - Error handling
- `serde` - JSON serialization
- `uuid` - Request ID generation

## License

MIT

## References

- [Stellar Horizon API Documentation](https://developers.stellar.org/api/introduction/index/)
- [Stellar Rate Limits](https://developers.stellar.org/api/introduction/rate-limiting/)
- [GitHub Repository](https://github.com/stellar/js-stellar-sdk)
