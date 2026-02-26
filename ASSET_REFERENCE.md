# Asset Management Quick Reference

## Core Types

```rust
// Asset configuration
pub struct StellarAsset {
    pub code: String,      // "XLM", "USDC", etc.
    pub issuer: String,    // Address or empty for native
    pub decimals: u32,     // 7 for XLM, 6 for others
}

// Asset information
pub struct AssetMetadata {
    pub code: String,
    pub name: String,
    pub organization: String,
    pub description: String,
    pub visuals: AssetVisuals,  // Icons and logos
    pub website: String,
}

// Asset visual assets
pub struct AssetVisuals {
    pub icon_url: String,   // 32x32 icon
    pub logo_url: String,   // High-res logo
    pub color: String,      // Brand color hex
}
```

## Common Operations

### 1. Get Asset by Code

```rust
use stellaraid_core::assets::AssetResolver;

if let Some(asset) = AssetResolver::resolve_by_code("USDC") {
    // Use asset...
}
```

### 2. Check if Asset is Supported

```rust
if AssetResolver::is_supported("XLM") {
    // Asset is supported
}
```

### 3. Get All Supported Codes

```rust
let codes = AssetResolver::supported_codes();
// ["XLM", "USDC", "NGNT", "USDT", "EURT"]
```

### 4. Get Asset with Metadata

```rust
if let Some((asset, metadata)) = AssetResolver::resolve_with_metadata("USDC") {
    println!("{}: {}", asset.code, metadata.name);
}
```

### 5. Validate an Asset

```rust
use stellaraid_core::assets::AssetValidator;

match AssetValidator::validate_complete(&asset) {
    Ok(()) => println!("Valid asset"),
    Err(e) => println!("Error: {:?}", e),
}
```

### 6. Convert Between Assets

```rust
use stellaraid_core::assets::PriceFeedProvider;

// Convert 100 XLM to USDC
if let Some(usdc_amount) = PriceFeedProvider::convert("XLM", "USDC", 100_000_000) {
    println!("USDC: {}", usdc_amount);
}
```

### 7. Get Asset Metadata

```rust
use stellaraid_core::assets::MetadataRegistry;

if let Some(metadata) = MetadataRegistry::get_by_code("USDC") {
    println!("Icon: {}", metadata.visuals.icon_url);
    println!("Website: {}", metadata.website);
}
```

### 8. List All Assets

```rust
use stellaraid_core::assets::AssetRegistry;

let assets = AssetRegistry::all_assets();
for asset in &assets {
    println!("{} ({} decimals)", asset.code, asset.decimals);
}
```

## Asset Details

| Code | Name | Decimals | Issuer |
|------|------|----------|--------|
| XLM | Stellar Lumens | 7 | (native) |
| USDC | USD Coin | 6 | GA5ZSEJYB... |
| NGNT | Nigerian Naira Token | 6 | GAUYTZ24A... |
| USDT | Tether | 6 | GBBD47UZQ2... |
| EURT | Euro Token | 6 | GAP5LETOV... |

## Error Handling

```rust
use stellaraid_core::assets::AssetValidationError;

match result {
    Ok(()) => { /* success */ }
    Err(AssetValidationError::UnsupportedAsset) => { /* asset not configured */ }
    Err(AssetValidationError::InvalidAssetCode) => { /* code format invalid */ }
    Err(AssetValidationError::InvalidIssuer) => { /* issuer format invalid */ }
    Err(AssetValidationError::IncorrectDecimals) => { /* wrong decimals */ }
    _ => { /* other errors */ }
}
```

## Module Structure

```
assets/
├── config.rs         → Asset configurations
├── metadata.rs       → Asset metadata and visuals
├── resolver.rs       → Asset resolution utilities
├── validation.rs     → Asset validation logic
├── price_feeds.rs    → Price feed integration
└── mod.rs           → Module aggregation
```

## Common Patterns

### Pattern 1: Validate User Asset Input

```rust
fn validate_user_asset(asset: &StellarAsset) -> Result<()> {
    AssetValidator::validate_complete(asset)
}
```

### Pattern 2: Get Asset Info for Display

```rust
fn display_asset(code: &str) {
    if let Some(metadata) = MetadataRegistry::get_by_code(code) {
        // Display metadata, icon, etc.
    }
}
```

### Pattern 3: Convert Amount

```rust
fn convert_amount(from_code: &str, to_code: &str, amount: i128) -> Option<i128> {
    PriceFeedProvider::convert(from_code, to_code, amount)
}
```

### Pattern 4: Enumerate All Assets

```rust
for code in &AssetResolver::supported_codes() {
    if let Some(asset) = AssetResolver::resolve_by_code(code) {
        // Process asset...
    }
}
```

## Important Notes

1. **XLM is native** - Has empty issuer string, 7 decimals
2. **Stablecoins** - USDC, NGNT, USDT, EURT all have 6 decimals
3. **Trust lines** - Non-native assets require trust line setup
4. **Icons available** - Via Trust Wallet assets repository
5. **No runtime changes** - Assets are configured at compile time

## Version Info

- **API Version**: 1.0
- **Asset Count**: 5
- **Last Updated**: 2026-02-26

---

For complete documentation, see [ASSET_MANAGEMENT.md](ASSET_MANAGEMENT.md)
