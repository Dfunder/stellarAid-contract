# Horizon Client Integration Guide

## Overview

This guide explains how to integrate the Stellar Horizon API client into the stellarAid core contract for accessing account information, transaction history, and other on-chain data.

## Architecture

```
┌─────────────────────────────────────┐
│   Soroban Smart Contract (Core)     │
├─────────────────────────────────────┤
│  Contract Methods:                  │
│  - get_account_balance()            │
│  - get_recent_transactions()        │
│  - validate_payment_received()      │
│  - monitor_donations()              │
└────────────┬────────────────────────┘
             │
             ├─────────────────────────────────────┐
             │                                     │
             v                                     v
┌──────────────────────────┐         ┌──────────────────────────┐
│  Horizon Client Library  │         │   HTTP Request Handler   │
│  (Tools Crate)          │         │   (reqwest + governor)   │
├──────────────────────────┤         └──────────────────────────┘
│ Components:              │
│ - HorizonClient (main)   │
│ - Error handling         │
│ - Rate limiting          │
│ - Retry logic            │
│ - Caching                │
│ - Health checks          │
└────────────┬─────────────┘
             │
             v
┌──────────────────────────────────────┐
│   Stellar Horizon API (Public)       │
│   https://horizon.stellar.org/       │
└──────────────────────────────────────┘
```

## Integration Points

### 1. Account Information Queries

**Use Case**: Retrieve donor/recipient account details

```rust
// In core/src/lib.rs
use stellaraid_tools::horizon_client::HorizonClient;

pub fn get_account_info(env: &Env, account_id: &str) -> Result<AccountInfo, String> {
    // Create client
    let client = HorizonClient::public()
        .map_err(|e| format!("Failed to create Horizon client: {}", e))?;

    // Query account
    let path = format!("/accounts/{}", account_id);
    env.storage()
        .instance()
        .set(&contract::DataKey::CachedAccountInfo(String::from_env(
            env,
            &account_id,
        )?), &true);

    Ok(AccountInfo {
        id: account_id.to_string(),
        // ... populate from HTTP response
    })
}
```

### 2. Transaction History

**Use Case**: Verify payment receipts, track donation history

```rust
pub fn get_account_transactions(
    env: &Env,
    account_id: &str,
    limit: u32,
) -> Result<Vec<Transaction>, String> {
    let client = HorizonClient::public()
        .map_err(|e| format!("Horizon client error: {}", e))?;

    let path = format!(
        "/accounts/{}/transactions?limit={}&order=desc",
        account_id, limit
    );

    // Request will be:
    // 1. Rate-limited to respect 72 req/hour
    // 2. Retried on transient failures
    // 3. Cached for 60 seconds (optional)
    let response = client.get(&path)
        .await
        .map_err(|e| format!("Query failed: {}", e))?;

    // Parse and return transactions
    Ok(vec![])
}
```

### 3. Asset Validation

**Use Case**: Verify donations are in supported assets (XLM, USDC, etc.)

```rust
pub fn validate_asset_payment(
    env: &Env,
    asset_code: &str,
    asset_issuer: &str,
) -> Result<bool, String> {
    let client = HorizonClient::public()?;

    // Query asset on Horizon
    let path = format!("/assets?asset_code={}&asset_issuer={}", asset_code, asset_issuer);
    let response = client.get(&path).await?;

    // Cross-reference with our supported assets
    use stellaraid_contract::assets::resolver::AssetResolver;
    let resolver = AssetResolver::new();

    Ok(resolver.is_supported(asset_code))
}
```

### 4. Payment Verification

**Use Case**: Confirm donations received before crediting

```rust
pub fn verify_payment(
    env: &Env,
    account_id: &str,
    transaction_hash: &str,
    expected_amount: &str,
) -> Result<bool, String> {
    let client = HorizonClient::public()?;

    // Get specific transaction
    let path = format!("/transactions/{}", transaction_hash);
    let tx_response = client.get(&path).await?;

    // Verify:
    // 1. Amount matches
    // 2. Destination is contract
    // 3. Asset is supported
    // 4. Timestamp is recent

    Ok(true) // After validation
}
```

### 5. Health Monitoring

**Use Case**: Ensure Horizon is available before processing donations

```rust
pub async fn check_horizon_health(env: &Env) -> Result<String, String> {
    use stellaraid_tools::horizon_client::health::{HorizonHealthChecker, HealthStatus};

    let client = HorizonClient::public()?;
    let checker = HorizonHealthChecker::new(Default::default());

    let result = checker.check(&client).await?;

    match result.status {
        HealthStatus::Healthy => {
            env.log().info("Horizon is healthy");
            Ok("healthy".to_string())
        }
        HealthStatus::Degraded => {
            env.log().warn(&format!("Horizon is degraded: {}ms", result.response_time_ms));
            Ok("degraded".to_string())
        }
        HealthStatus::Unhealthy => {
            env.log().error(&format!("Horizon is down: {:?}", result.error));
            Err("Horizon API unavailable".to_string())
        }
        HealthStatus::Unknown => {
            Err("Cannot determine Horizon status".to_string())
        }
    }
}
```

## Configuration

### Public Horizon (Default)

```rust
let client = HorizonClient::public()?;

// Respects:
// - 72 requests/hour rate limit
// - 30s timeout
// - Exponential backoff retry (100ms -> 30s)
// - 60s response caching
// - Request logging
```

### Private Horizon (Custom)

```rust
let client = HorizonClient::private(
    "https://my-horizon.example.com",
    1000.0  // requests per second
)?;

// Use for testing or private networks
```

### Test Configuration

```rust
let config = HorizonClientConfig::test();
let client = HorizonClient::with_config(config)?;

// Disables:
// - Rate limiting
// - Retries
// - Caching
// - Logging
```

## Error Handling

### Retryable Errors

The client automatically retries on transient failures:

- **Network errors** (DNS, connection refused, connection reset)
- **Timeouts** (request took too long)
- **Server errors** (5xx responses)
- **Rate limiting** (429 Too Many Requests)

Example handling:

```rust
match client.get("/accounts/...").await {
    Ok(response) => {
        // Process response
    }
    Err(e) if e.is_retryable() => {
        // This was already retried automatically
        // Log and decide on application-level action
        env.log().warn(&format!("User should retry later: {}", e));
    }
    Err(e) if e.is_client_error() => {
        // 4xx error - don't retry
        return Err(format!("Invalid request: {}", e));
    }
    Err(e) => {
        // Other errors
        return Err(format!("Failed: {}", e));
    }
}
```

### Rate Limit Handling

When rate limited:

```rust
if let HorizonError::RateLimited { retry_after } = error {
    env.log().info(&format!("Rate limited, retry after {:?}", retry_after));
    // Wait and retry manually if needed
    tokio::time::sleep(retry_after).await;
    let retry = client.get(path).await?;
}
```

## Caching Strategy

### Enable Caching

```rust
let config = HorizonClientConfig {
    enable_cache: true,
    cache_ttl: Duration::from_secs(300), // 5 minutes
    ..Default::default()
};

let client = HorizonClient::with_config(config)?;
```

### Cache Statistics

```rust
let stats = client.cache_stats().await?;

println!("Cache entries: {}", stats.entries);
println!("Cache hits: {}", stats.hits);
println!("Cache misses: {}", stats.misses);
println!("Hit rate: {:.2}%", stats.hit_rate() * 100.0);
```

### When to Use Caching

**Enable for:**
- Account information queries (stable, frequently accessed)
- Asset list queries (rarely changes)
- Account balance checks (read-heavy operations)

**Disable for:**
- Real-time transaction verification (needs current data)
- Payment confirmations (needs fresh data)

## Rate Limiting Details

### Public Horizon: 72 requests per hour

```
72 requests / 3600 seconds = 0.02 requests/second
= ~50 millisecond delay between requests

Max burst: 1 request every 50ms
```

### Respecting Rate Limits

The client handles this transparently:

```rust
// These 3 requests are automatically spaced out
client.get("/ledgers").await?;        // Immediate
client.get("/transactions").await?;   // Waits ~50ms
client.get("/accounts/...").await?;   // Waits ~50ms
```

### Rate Limit Statistics

```rust
let stats = client.rate_limiter_stats();

println!("Requests per hour: {}", stats.config.requests_per_hour);
println!("Time until ready: {:?}", stats.time_until_ready);
println!("Ready for request: {}", stats.is_ready());
```

## Monitoring and Logging

### Enable Debug Logging

```rust
// In your application startup
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
    .init();

// Or set environment variable
// export RUST_LOG=stellaraid_tools=debug
```

### Log Output

Each request generates logs:

```
[DEBUG] Horizon: Starting request [UUID:12ab-34cd] GET /accounts/GXXXXXXX
[DEBUG] Horizon: Rate limited, waiting 50ms [UUID:12ab-34cd]
[DEBUG] Horizon: Retrying after network error (attempt 1/3) [UUID:12ab-34cd]
[DEBUG] Horizon: Request succeeded in 245ms [UUID:12ab-34cd]
```

### Health Monitoring

Continuous background monitoring:

```rust
let monitor = HealthMonitor::new(checker, 60); // Check every 60 seconds
monitor.start(client.clone()).await;

// Later:
monitor.stop();
```

## Testing

### Unit Tests

```bash
cargo test --lib horizon_client
```

### Integration Tests

```bash
# Against mock/local Horizon
cargo test --test horizon_client_integration

# Against testnet (slow, requires network)
cargo test --test horizon_client_integration -- --ignored --nocapture
```

### Example Implementation

See `examples/horizon_client_examples.rs` for 12 complete usage patterns.

## Performance Considerations

### Request Pooling

The HTTP client uses connection pooling automatically:

```rust
// All requests share the same connection pool
let client = HorizonClient::public()?;

// These are efficient - no reconnects
for account in accounts {
    let path = format!("/accounts/{}", account);
    client.get(&path).await?;
}
```

### Caching Impact

With 60-second caching:

```
Without cache:
- 100 account queries = 100 HTTP requests
- Time: 100 * 50ms = 5 seconds (rate limited)

With cache:
- 100 account queries = 1-2 HTTP requests
- Time: ~100ms (rest from cache)
```

### Memory Usage

- Default cache: ~256MB max entries
- Each cached response: ~2-5KB
- ~50,000 entries before eviction

## Troubleshooting

### Rate Limit Exceeded

**Problem**: Getting `RateLimited` errors frequently

**Solution**:
1. Check request volume vs 72/hour limit
2. Enable caching for duplicate queries
3. Batch requests when possible
4. Use private Horizon with higher limit

### Timeout Errors

**Problem**: Requests timing out after 30 seconds

**Solution**:
1. Increase timeout: `timeout: Duration::from_secs(60)`
2. Check network latency to Horizon
3. Downgrade to fewer fields requested
4. Use pagination for large result sets

### Connection Refused

**Problem**: Cannot connect to Horizon

**Solution**:
1. Verify network connectivity
2. Check firewall rules
3. Use health check to diagnose
4. Fall back to degraded mode

### Out of Memory

**Problem**: Cache growing too large

**Solution**:
1. Disable caching: `enable_cache: false`
2. Reduce TTL: `cache_ttl: Duration::from_secs(30)`
3. Clear cache periodically: `client.clear_cache()`
4. Monitor: `client.cache_stats()`

## Future Enhancements

### Planned

1. **WebSocket support** - Real-time transaction streaming
2. **Connection pooling per endpoint** - Optimize concurrent request patterns
3. **Request deduplication** - Automatically merge duplicate in-flight requests
4. **Custom circuit breaker** - Deeper failure detection
5. **Metrics export** - Prometheus format for monitoring

### Experimental

1. **GraphQL queries** - More flexible queries
2. **Subscription webhooks** - Event-based notifications
3. **Local caching** - Persistent cache across restarts

## References

- [Stellar Horizon API Documentation](https://developers.stellar.org/)
- [API Rate Limiting](https://developers.stellar.org/docs/build/rate-limiting)
- [API Endpoints Reference](https://developers.stellar.org/api/)

## Support

For issues or questions:

1. Check this guide first
2. Review `HORIZON_CLIENT.md` for detailed documentation
3. Look at examples in `examples/horizon_client_examples.rs`
4. Check test cases in `crates/tools/tests/horizon_client_integration.rs`
5. Review logs with `RUST_LOG=debug`
