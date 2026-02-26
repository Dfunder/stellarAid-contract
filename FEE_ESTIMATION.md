# Fee Estimation Service Documentation

## Overview

The Fee Estimation Service provides comprehensive utilities for managing Stellar transaction fees. It handles fee calculation, surge pricing detection, currency conversion, caching, and historical tracking.

### Acceptance Criteria Fulfillment

- ✅ Accurate fee estimates provided
- ✅ Fees update based on network conditions
- ✅ Users see fees before confirming transactions
- ✅ Fees converted to display currency
- ✅ Surge pricing detected and shown

## Architecture

### Core Components

#### 1. Fee Estimation Service (`service.rs`)
Main orchestrator that integrates all fee management features.

**Key Features:**
- Fetches current base fees from Stellar Horizon
- Caches fees with 5-minute TTL (configurable)
- Detects surge pricing automatically
- Converts fees to user's preferred currency
- Tracks fee history for analysis

**Public API:**
```rust
// Create service for public Horizon
let service = FeeEstimationService::public_horizon();

// Estimate fee for operations
let fee_info = service.estimate_fee(operation_count).await?;

// Estimate with currency conversion
let (fee_info, converted) = service.estimate_fee_in_currency(
    operation_count,
    Currency::USD
).await?;

// Get fee statistics
let stats = service.get_fee_stats().await;
```

#### 2. Fee Calculator (`calculator.rs`)
Handles fee calculations and conversions.

**Key Constants:**
- Base fee: **100 stroops** (0.00001 XLM)
- 1 XLM = **10,000,000 stroops**
- Fee = base_fee × operation_count

**Examples:**
```rust
use fee::calculator::*;

// Single operation
let fee = calculate_fee(100, 1)?; // 100 stroops
let xlm = stroops_to_xlm(100);    // 0.00001 XLM

// Multiple operations (5 ops)
let fee = calculate_fee(100, 5)?; // 500 stroops
let xlm = stroops_to_xlm(500);    // 0.00005 XLM
```

#### 3. Surge Pricing Detector (`surge_pricing.rs`)
Identifies network congestion and fee spikes.

**Surge Levels:**
- **Normal** (0-100%): Network fee is at baseline
- **Elevated** (100-150%): Slight congestion, fees up 0-50%
- **High** (150-300%): Moderate congestion, fees up 50-200%
- **Critical** (>300%): Severe congestion, fees >200% above normal

**Example:**
```rust
use fee::surge_pricing::*;

let config = SurgePricingConfig::default();
let mut analyzer = SurgePricingAnalyzer::new(config);

let analysis = analyzer.analyze(250)?; // 250 stroops base fee
println!("Surge level: {}", analysis.surge_level.name());  // "High"
println!("Surge percent: {}%", analysis.surge_percent);     // 250%
println!("Recommendation: {}", analysis.recommendation);
// Output: "Network is congested. Consider waiting if not urgent."
```

#### 4. Fee Cache (`cache.rs`)
Manages fee caching with TTL support (default: 5 minutes).

**Cache Strategy:**
- Fetches from Horizon only when cache expires
- Reduces API calls and improves performance
- Configurable TTL (300 seconds default)

**Example:**
```rust
use fee::cache::*;

let mut cache = FeeCache::default_ttl();
cache.set(100)?;                    // Store fee

if let Some(fee) = cache.get() {
    println!("Cached fee: {}", fee);
}

let metadata = cache.metadata();
println!("Cache expires in: {}s", metadata.time_until_expiration);
```

#### 5. Currency Converter (`currency.rs`)
Converts fees between XLM and fiat currencies.

**Supported Currencies:**
- **Cryptocurrencies**: XLM
- **Major Fiat**: USD, EUR, GBP, JPY

**Example:**
```rust
use fee::currency::*;

let mut converter = CurrencyConverter::new();

// Set exchange rate (1 XLM = 0.25 USD)
converter.set_rate(Currency::XLM, Currency::USD, 0.25)?;

// Convert fee
let usd_amount = converter.convert_xlm_fee(1.0, Currency::USD)?;
println!("Fee in USD: {}", usd_amount);  // 0.25

// Format for display
let formatted = FormattedAmount::new(0.25, Currency::USD);
println!("{}", formatted.to_string_precision(2));  // "$ 0.25"
```

#### 6. Fee History Tracker (`history.rs`)
Maintains historical fee records for analysis.

**Features:**
- Tracks up to 1000 fee observations (configurable)
- Calculates statistics (min, max, avg, median, std dev)
- Analyzes fee trends
- Detects maximum fee changes

**Example:**
```rust
use fee::history::*;

let mut history = FeeHistory::default_capacity();
history.add(100, "Horizon".to_string())?;
history.add(150, "Horizon".to_string())?;
history.add(200, "Horizon".to_string())?;

let stats = history.stats().unwrap();
println!("Min: {}", stats.min_fee);        // 100
println!("Max: {}", stats.max_fee);        // 200
println!("Average: {:.0}", stats.avg_fee); // 150
println!("Median: {}", stats.median_fee);  // 150

// Calculate max change over last hour
let change_percent = history.max_change_percent(3600);
println!("Max change in 1h: {}%", change_percent);
```

#### 7. Horizon Fee Fetcher (`horizon_fetcher.rs`)
Fetches current base fees from Stellar Horizon API.

**Endpoints:**
- Public Horizon: `https://horizon.stellar.org`
- Custom servers supported

**Example:**
```rust
use fee::horizon_fetcher::*;

let fetcher = HorizonFeeFetcher::public_horizon();
let base_fee = fetcher.fetch_base_fee().await?;
println!("Current base fee: {} stroops", base_fee);
```

## Fee Calculation Logic

### Basic Formula

```
total_fee = base_fee × operation_count
```

Where:
- `base_fee` = current network base fee in stroops (typically 100)
- `operation_count` = number of operations in the transaction

### Multi-Currency Support

1. **Calculate base fee in stroops**: Formula above
2. **Convert to XLM**: `fee_stroops ÷ 10,000,000`
3. **Apply exchange rate**: `fee_xlm × exchange_rate`

### Example Calculation

**Scenario:** 2-operation donation with 1 XLM = $0.25 USD rate

```
Step 1: Calculate stroops
  total_fee = 100 stroops/op × 2 ops = 200 stroops

Step 2: Convert to XLM
  fee_xlm = 200 ÷ 10,000,000 = 0.00002 XLM

Step 3: Convert to USD
  fee_usd = 0.00002 × 0.25 = 0.000005 USD

Display: "$0.000005" or "0.00002 XLM"
```

### Surge Pricing Calculation

```
surge_percent = (current_base_fee / normal_base_fee) × 100

Where:
  normal_base_fee = 100 (typical)
  current_base_fee = fee observed from Horizon
```

**Examples:**
- 100 stroops → 100% (normal)
- 150 stroops → 150% (50% surge)
- 250 stroops → 250% (150% surge)
- 500 stroops → 500% (400% surge)

## Integration Guide

### Basic Usage

```rust
use fee::{FeeEstimationService, Currency};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create service
    let service = FeeEstimationService::public_horizon();

    // Estimate fee for 3-operation transaction
    let fee_info = service.estimate_fee(3).await?;
    
    println!("Fee: {} XLM", fee_info.total_fee_xlm);
    println!("Fee in stroops: {}", fee_info.total_fee_stroops);
    
    // Check for surge pricing
    if fee_info.is_surge_pricing {
        println!("⚠️ Network surging at {}%!", fee_info.surge_percent as i64);
    }

    Ok(())
}
```

### With Currency Conversion

```rust
use fee::{FeeEstimationService, Currency};

let service = FeeEstimationService::public_horizon();

// Set exchange rate (1 XLM = $0.25)
service.set_exchange_rate(
    Currency::XLM,
    Currency::USD,
    0.25,
).await?;

// Get fee in USD
let (fee_info, usd_amount) = service
    .estimate_fee_in_currency(2, Currency::USD)
    .await?;

println!("Fee: ${}", usd_amount);
```

### Display in UI

```rust
// For donation confirmation modal
let fee_info = service.estimate_fee(operation_count).await?;

let fee_display = if fee_info.is_surge_pricing {
    format!(
        "Network Fee: {:.6} XLM (⚠️ +{}% surge)",
        fee_info.total_fee_xlm,
        (fee_info.surge_percent - 100.0) as i64
    )
} else {
    format!("Network Fee: {:.6} XLM", fee_info.total_fee_xlm)
};

println!("{}", fee_display);
```

## Configuration

### Default Configuration

```rust
use fee::service::FeeServiceConfig;

let config = FeeServiceConfig::default();
// horizon_url: "https://horizon.stellar.org"
// cache_ttl_secs: 300 (5 minutes)
// fetch_timeout_secs: 30
// max_history_records: 1000
// enable_surge_detection: true
```

### Custom Configuration

```rust
let config = FeeServiceConfig {
    horizon_url: "https://my-horizon.example.com".to_string(),
    cache_ttl_secs: 600,           // 10 minutes
    fetch_timeout_secs: 60,
    max_history_records: 5000,
    enable_surge_detection: true,
};

let service = FeeEstimationService::new(config);
```

## Error Handling

### Error Types

```rust
pub enum FeeError {
    HorizonUnavailable(String),        // Network unreachable
    InvalidFeeValue(String),           // Fee < 0 or invalid
    CurrencyConversionFailed(String),  // No exchange rate
    InvalidCurrency(String),           // Unknown currency
    CacheUnavailable(String),          // Cache error
    InvalidOperationCount(String),     // Op count = 0
    NetworkError(String),              // Network issues
    ParseError(String),                // JSON parsing failed
    InvalidConfig(String),             // Config problem
    Timeout,                           // Horizon timeout
    Other(String),
}
```

### Error Handling Example

```rust
match service.estimate_fee(operation_count).await {
    Ok(fee_info) => {
        println!("Fee: {}", fee_info.total_fee_xlm);
    }
    Err(FeeError::Timeout) => {
        println!("Horizon request timed out, using cached fee");
        // Fall back to cached fee
    }
    Err(FeeError::HorizonUnavailable(_)) => {
        println!("Horizon unavailable, using last known fee");
        // Fall back to previous estimate
    }
    Err(e) => {
        eprintln!("Fee estimation failed: {}", e);
    }
}
```

## Performance Characteristics

### Time Complexity
- **Fee calculation**: O(1)
- **Cache lookup**: O(1)
- **Currency conversion**: O(1)
- **History stats**: O(n) where n = history size
- **Surge detection**: O(1)

### Space Complexity
- **Cache**: O(1) - single entry
- **History**: O(max_records) - configurable, default 1000
- **Exchange rates**: O(currency_pairs) - typically < 100

### Network Calls
- **First call**: 1 HTTP request to Horizon
- **Cached calls** (< 5m): 0 HTTP requests
- **Expired cache**: 1 HTTP request to Horizon

## Testing

### Unit Tests

All modules include comprehensive unit tests:

```bash
# Run all fee tests
cargo test -p tools fee

# Run specific module tests
cargo test -p tools fee::calculator
cargo test -p tools fee::surge_pricing
cargo test -p tools fee::currency
```

### Integration Testing

Create integration tests for end-to-end fee flow:

```rust
#[tokio::test]
async fn test_fee_estimation_e2e() {
    let service = FeeEstimationService::public_horizon();
    
    // Should not panic and return valid estimate
    let fee_info = service.estimate_fee(1).await.unwrap();
    assert!(fee_info.total_fee_stroops > 0);
    assert!(fee_info.total_fee_xlm > 0.0);
}
```

## Troubleshooting

### High Fees
**Symptom:** Fee significantly higher than expected

**Causes:**
- Network surge pricing (check `fee_info.is_surge_pricing`)
- Multiple operations required
- Using private Horizon with higher fees

**Solution:**
- Wait for network congestion to decrease
- Reduce number of operations if possible
- Keep transaction offline-ready before signing

### Cache Not Updating
**Symptom:** Fee remains unchanged despite network changes

**Cause:**
- Cache TTL not expired (default 5 minutes)
- Service not fetching from Horizon

**Solution:**
```rust
// Clear cache to force refresh
service.clear_cache().await;
```

### Conversion Errors
**Symptom:** Currency conversion fails

**Cause:**
- Exchange rate not set for currency pair
- Invalid currency code

**Solution:**
```rust
// Ensure exchange rate is set
service.set_exchange_rate(
    Currency::XLM,
    Currency::USD,
    current_rate
).await?;
```

## Best Practices

1. **Cache Management**
   - Use default 5-minute TTL for most applications
   - Clear cache manually only when necessary
   - Monitor cache hit rates in production

2. **Error Handling**
   - Fall back to cached fees if Horizon unavailable
   - Display cache age to user if possible
   - Log all network errors for diagnostics

3. **UI Display**
   - Always show fee before user confirms
   - Highlight surge pricing clearly
   - Display in user's preferred currency

4. **Exchange Rates**
   - Update rates frequently (every 1-5 minutes)
   - Use trusted price feeds
   - Handle missing rates gracefully

5. **History Tracking**
   - Keep history for analytics
   - Analyze trends over time
   - Detect unusual fee behavior

## API Reference

### FeeEstimationService

```rust
impl FeeEstimationService {
    // Creation
    pub fn new(config: FeeServiceConfig) -> Self
    pub fn public_horizon() -> Self
    
    // Fee estimation
    pub async fn estimate_fee(&self, operation_count: u32) -> FeeResult<FeeInfo>
    pub async fn estimate_fee_in_currency(
        &self,
        operation_count: u32,
        currency: Currency
    ) -> FeeResult<(FeeInfo, f64)>
    pub async fn batch_estimate_fees(&self, counts: &[u32]) -> FeeResult<Vec<FeeInfo>>
    
    // Currency conversion
    pub async fn set_exchange_rate(
        &self,
        from: Currency,
        to: Currency,
        rate: f64
    ) -> FeeResult<()>
    
    // Statistics
    pub async fn get_fee_stats(&self) -> Option<FeeStats>
    pub async fn get_recent_fee_stats(&self, seconds: i64) -> Option<FeeStats>
    pub async fn get_surge_info(&self) -> Option<String>
    pub async fn is_surging(&self) -> FeeResult<bool>
    
    // Caching
    pub async fn clear_cache(&self)
    pub async fn get_cache_metadata(&self) -> Option<CacheMetadata>
    
    // History
    pub async fn clear_history(&self)
    pub async fn get_history_count(&self) -> usize
}
```

## Examples

### Example 1: Donation Fee Estimate

```rust
// Get fee for donation (2 operations: payment + contract invoke)
let service = FeeEstimationService::public_horizon();
let fee_info = service.estimate_fee(2).await?;

println!("Donation will cost:");
println!("  XLM: {:.8}", fee_info.total_fee_xlm);
println!("  Stroops: {}", fee_info.total_fee_stroops);

if fee_info.is_surge_pricing {
    println!("⚠️  Network surging! Fees are {:.0}% above normal", 
             fee_info.surge_percent);
}
```

### Example 2: Multi-Currency Display

```rust
let service = FeeEstimationService::public_horizon();

// Set exchange rates
service.set_exchange_rate(Currency::XLM, Currency::USD, 0.25).await?;
service.set_exchange_rate(Currency::XLM, Currency::EUR, 0.23).await?;

let (fee_info, usd) = service.estimate_fee_in_currency(2, Currency::USD).await?;
let (_, eur) = service.estimate_fee_in_currency(2, Currency::EUR).await?;

println!("Network Fee:");
println!("  {:.8} XLM", fee_info.total_fee_xlm);
println!("  ${:.6}", usd);
println!("  €{:.6}", eur);
```

### Example 3: Batch Fee Estimates

```rust
let service = FeeEstimationService::public_horizon();

// Estimate fees for different operation counts
let counts = vec![1, 2, 3, 5, 10];
let fees = service.batch_estimate_fees(&counts).await?;

for (count, fee) in counts.into_iter().zip(fees) {
    println!("{} ops: {:.8} XLM", count, fee.total_fee_xlm);
}
```

## See Also

- [Horizon API Documentation](https://developers.stellar.org/api/)
- [Stellar Fees Documentation](https://developers.stellar.org/learn/fundamentals/fees-and-pricing)
- [Surge Pricing Details](https://developers.stellar.org/learn/fundamentals/fees-and-pricing#surge-pricing)

---

**Last Updated:** 2026-02-26
**Version:** 1.0
