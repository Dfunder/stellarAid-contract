# Stellar Asset Management System

This documentation describes the comprehensive asset management system for handling Stellar assets in the StellarAid contract.

## Overview

The asset management system provides:

- **Asset Configuration** (`config.rs`) - Centralized definitions for all supported Stellar assets
- **Asset Resolution** (`resolver.rs`) - Utilities to resolve and validate assets
- **Asset Metadata** (`metadata.rs`) - Visual assets, icons, and descriptive information
- **Asset Validation** (`validation.rs`) - Validation logic for assets and trust lines
- **Price Feed Integration** (`price_feeds.rs`) - Optional price feed and conversion rate management

## Supported Assets

### 1. XLM (Stellar Lumens)
- **Code**: XLM
- **Issuer**: Native (no issuer address)
- **Decimals**: 7
- **Organization**: Stellar Development Foundation
- **Use**: Native currency of Stellar network

### 2. USDC (USD Coin)
- **Code**: USDC
- **Issuer**: `GA5ZSEJYB37JRC5AVCIA5MOP4GZ5DA47EL4PMRV4ZU5KHSUCZMVDXEN`
- **Decimals**: 6
- **Organization**: Circle
- **Use**: Stablecoin backed by US Dollar

### 3. NGNT (Nigerian Naira Token)
- **Code**: NGNT
- **Issuer**: `GAUYTZ24ATZTPC35NYSTSIHIVGZSC5THJOsimplicc4B3TDTFSLOMNLDA`
- **Decimals**: 6
- **Organization**: Stellar Foundation
- **Use**: Stablecoin for Nigerian Naira

### 4. USDT (Tether)
- **Code**: USDT
- **Issuer**: `GBBD47UZQ2EOPIB6NYVTG2ND4VS4F7IJDLLUOYRCG76K7JT45XE7VAT`
- **Decimals**: 6
- **Organization**: Tether Limited
- **Use**: Original stablecoin

### 5. EURT (Euro Token)
- **Code**: EURT
- **Issuer**: `GAP5LETOV6YIE272RLUBZTV3QQF5JGKZ5FWXVMMP4QSXG7GSTF5GNBE7`
- **Decimals**: 6
- **Organization**: Wirex
- **Use**: Euro stablecoin

## API Reference

### Asset Configuration (`assets::config`)

#### `StellarAsset`
Represents a Stellar asset with code, issuer, and decimal information.

```rust
pub struct StellarAsset {
    pub code: String,      // Asset code (e.g., "XLM", "USDC")
    pub issuer: String,    // Issuer address (empty for native)
    pub decimals: u32,     // Number of decimal places
}
```

**Methods:**
- `is_xlm()` - Check if this is the native XLM asset
- `id()` - Get unique identifier for the asset

#### `AssetRegistry`
Static registry for all supported assets.

**Methods:**
- `xlm()` - Get XLM asset configuration
- `usdc()` - Get USDC asset configuration
- `ngnt()` - Get NGNT asset configuration
- `usdt()` - Get USDT asset configuration
- `eurt()` - Get EURT asset configuration
- `all_assets()` - Get array of all assets
- `all_codes()` - Get array of all asset codes

### Asset Resolution (`assets::resolver`)

#### `AssetResolver`
Utility for resolving and validating Stellar assets.

**Methods:**
- `resolve_by_code(code)` - Resolve asset by its code
- `is_supported(code)` - Check if asset code is supported
- `supported_codes()` - Get list of supported codes
- `count()` - Get total count of supported assets
- `matches(code, issuer, asset)` - Check if asset matches configuration
- `resolve_with_metadata(code)` - Get asset with metadata
- `validate(asset)` - Validate asset against configuration

**Example:**
```rust
use stellaraid_core::assets::AssetResolver;

// Resolve USDC
if let Some(usdc) = AssetResolver::resolve_by_code("USDC") {
    println!("USDC decimals: {}", usdc.decimals);
}

// Check if supported
if AssetResolver::is_supported("XLM") {
    println!("XLM is supported!");
}

// Get supported codes
let codes = AssetResolver::supported_codes();
for code in &codes {
    println!("Supported: {}", code);
}
```

### Asset Metadata (`assets::metadata`)

#### `AssetMetadata`
Complete metadata about an asset including visuals.

```rust
pub struct AssetMetadata {
    pub code: String,
    pub name: String,
    pub organization: String,
    pub description: String,
    pub visuals: AssetVisuals,
    pub website: String,
}
```

#### `AssetVisuals`
Visual assets for an asset.

```rust
pub struct AssetVisuals {
    pub icon_url: String,    // 32x32 icon
    pub logo_url: String,    // High-resolution logo
    pub color: String,       // Brand color in hex
}
```

#### `MetadataRegistry`
Static registry for asset metadata.

**Methods:**
- `xlm()` - Get XLM metadata
- `usdc()` - Get USDC metadata
- `ngnt()` - Get NGNT metadata
- `usdt()` - Get USDT metadata
- `eurt()` - Get EURT metadata
- `get_by_code(code)` - Get metadata by asset code
- `all()` - Get all metadata entries

**Example:**
```rust
use stellaraid_core::assets::MetadataRegistry;

if let Some(metadata) = MetadataRegistry::get_by_code("USDC") {
    println!("Asset: {}", metadata.name);
    println!("Organization: {}", metadata.organization);
    println!("Icon: {}", metadata.visuals.icon_url);
}
```

### Asset Validation (`assets::validation`)

#### `AssetValidator`
Comprehensive asset validation utilities.

**Methods:**
- `validate_asset(asset)` - Validate asset is supported
- `is_valid_asset_code(code)` - Check if code is valid format
- `is_valid_issuer(issuer)` - Check if issuer is valid format
- `verify_decimals(asset)` - Verify correct decimal places
- `validate_complete(asset)` - Perform complete validation

**Example:**
```rust
use stellaraid_core::assets::{AssetValidator, AssetRegistry};

let asset = AssetRegistry::usdc();

// Validate the asset
match AssetValidator::validate_complete(&asset) {
    Ok(()) => println!("Asset is valid!"),
    Err(e) => println!("Validation error: {:?}", e),
}
```

### Price Feed Integration (`assets::price_feeds`)

#### `PriceData`
Price information for an asset.

```rust
pub struct PriceData {
    pub asset_code: String,  // e.g., "XLM"
    pub price: i128,         // Price value
    pub decimals: u32,       // Decimal places
    pub timestamp: u64,      // Unix timestamp
    pub source: String,      // e.g., "coingecko"
}
```

#### `ConversionRate`
Conversion rate between two assets.

```rust
pub struct ConversionRate {
    pub from_asset: String,  // Source asset code
    pub to_asset: String,    // Target asset code
    pub rate: i128,          // Conversion rate
    pub decimals: u32,       // Decimal places
    pub timestamp: u64,      // Unix timestamp
}
```

#### `PriceFeedProvider`
Price feed operations.

**Methods:**
- `get_price(asset_code)` - Get current price of asset
- `get_conversion_rate(from, to)` - Get conversion rate between assets
- `convert(from, to, amount)` - Convert amount between assets
- `is_price_fresh(price, max_age, current_time)` - Check if price is current
- `validate_price(price)` - Validate price data integrity

**Example:**
```rust
use stellaraid_core::assets::PriceFeedProvider;

// Convert 100 XLM to USDC
if let Some(amount_usdc) = PriceFeedProvider::convert("XLM", "USDC", 100_000_000) {
    println!("100 XLM = {} USDC", amount_usdc);
}
```

## Integration Examples

### Example 1: Validating User Input Asset

```rust
use stellaraid_core::assets::{StellarAsset, AssetValidator, AssetResolver};
use soroban_sdk::{String, Env};

fn validate_user_asset(env: &Env, asset: &StellarAsset) -> Result<(), String> {
    // Check if asset is supported
    if !AssetResolver::validate(asset) {
        return Err(String::from_str(env, "Unsupported asset"));
    }

    // Validate complete structure
    AssetValidator::validate_complete(asset)
        .map_err(|_| String::from_str(env, "Invalid asset"))?;

    Ok(())
}
```

### Example 2: Getting Asset Information

```rust
use stellaraid_core::assets::{AssetResolver, MetadataRegistry};

fn get_asset_info(code: &str) -> Result<(StellarAsset, AssetMetadata), String> {
    AssetResolver::resolve_with_metadata(code)
        .ok_or_else(|| format!("Asset {} not found", code))
}
```

### Example 3: Converting Between Assets

```rust
use stellaraid_core::assets::PriceFeedProvider;

fn convert_to_usdc(from_code: &str, amount: i128) -> Option<i128> {
    PriceFeedProvider::convert(from_code, "USDC", amount)
}

// Usage
let xlm_amount = 100_000_000; // 100 XLM
if let Some(usdc_amount) = convert_to_usdc("XLM", xlm_amount) {
    println!("USDC equivalent: {}", usdc_amount);
}
```

### Example 4: Enumerating Supported Assets

```rust
use stellaraid_core::assets::AssetResolver;

fn list_supported_assets() {
    let codes = AssetResolver::supported_codes();
    for code in &codes {
        if let Some(asset) = AssetResolver::resolve_by_code(code) {
            println!("- {} (decimals: {})", asset.code, asset.decimals);
        }
    }
}
```

## Adding New Assets

To add a new supported asset:

1. **Add to config.rs**:
   - Add new method to `AssetRegistry` struct
   - Add asset code to `all_codes()` array
   - Add asset to `all_assets()` array

2. **Add to metadata.rs**:
   - Add new method to `MetadataRegistry` struct
   - Include icon URLs and branding info
   - Update `get_by_code()` match statement
   - Add to `all()` array

3. **Add to resolver.rs**:
   - Update `resolve_by_code()` match statement
   - Update `is_supported()` match statement

4. **Add to validation.rs**:
   - Update `verify_decimals()` decimal verification
   - Update validation logic as needed

5. **Update tests**:
   - Add test cases in each module

## Testing

All modules include comprehensive test suites:

```bash
# Run all tests
cargo test --all

# Run tests for specific module
cargo test assets::config
cargo test assets::resolver
cargo test assets::validation

# Run tests with output
cargo test -- --nocapture
```

## Decimals Configuration

Asset decimals determine how prices and amounts are represented:

- **XLM**: 7 decimals (smallest unit: 0.0000001 XLM)
- **USDC, NGNT, USDT, EURT**: 6 decimals (smallest unit: 0.000001)

When performing calculations:
```rust
// For USDC with 6 decimals
let amount = 100_000_000; // Represents 100 USDC
let in_cents = amount / 10_000; // Convert to cents
```

## Performance Considerations

1. **Asset Resolution**: O(1) - Direct code lookup
2. **Validation**: O(1) - Fixed number of checks
3. **Metadata Lookup**: O(1) - Direct code matching
4. **Price Feed Operations**: Depends on oracle, but generally O(1)

## Security Considerations

1. **Issuer Validation**: Always verify issuer addresses against configuration
2. **Decimal Safety**: Validate decimals to prevent rounding errors
3. **Price Feed Trust**: Only use trusted oracle sources
4. **Amount Validation**: Check for overflow/underflow in conversions

## Future Enhancements

- [ ] Dynamic asset registry with on-chain updates
- [ ] Multiple oracle sources with fallback logic
- [ ] Historical price tracking
- [ ] Integration with Soroswap for liquidity data
- [ ] Automated asset discovery from trusted registries
- [ ] Custom asset support with governance

## References

- [Stellar Assets](https://developers.stellar.org/docs/learn/concepts/assets)
- [Asset Codes](https://developers.stellar.org/docs/learn/concepts/assets#asset-code)
- [Trust Lines](https://developers.stellar.org/docs/learn/concepts/trustlines)
