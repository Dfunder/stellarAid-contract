# Asset Management Integration Guide

This guide shows how to integrate the asset management system into existing contract functions.

## Overview

The asset management system can be integrated into contract methods to:
- Validate user-provided assets
- Get asset information for responses
- Perform asset conversions
- Track asset balances
- Validate trust lines

## Integration Patterns

### Pattern 1: Asset-Specific Contract Method

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use crate::assets::{AssetResolver, AssetValidator};

#[contractimpl]
impl CoreContract {
    /// Get information about a supported asset
    pub fn get_asset_info(env: Env, code: String) -> Result<AssetInfo, String> {
        let code_str = std::str::from_utf8(code.as_raw().as_slice())
            .map_err(|_| String::from_str(&env, "Invalid asset code"))?;

        let (asset, metadata) = AssetResolver::resolve_with_metadata(code_str)
            .ok_or_else(|| String::from_str(&env, "Asset not supported"))?;

        Ok(AssetInfo {
            code: asset.code,
            issuer: asset.issuer,
            decimals: asset.decimals,
            name: metadata.name,
            organization: metadata.organization,
        })
    }
}
```

### Pattern 2: Validate Asset in Contract Call

```rust
use soroban_sdk::contractimpl;
use crate::assets::{AssetValidator, StellarAsset};

#[contractimpl]
impl CoreContract {
    /// Transfer specified asset
    pub fn transfer_asset(
        env: Env,
        asset: StellarAsset,
        to: Address,
        amount: i128,
    ) -> Result<(), String> {
        // Validate asset is supported
        AssetValidator::validate_complete(&asset)
            .map_err(|_| String::from_str(&env, "Invalid asset"))?;

        // Continue with transfer logic...
        Ok(())
    }
}
```

### Pattern 3: List Supported Assets

```rust
use soroban_sdk::contractimpl;
use crate::assets::{AssetResolver, MetadataRegistry};

#[contractimpl]
impl CoreContract {
    /// Get list of all supported assets
    pub fn list_supported_assets(env: Env) -> Vec<SupportedAsset> {
        AssetResolver::supported_codes()
            .iter()
            .filter_map(|code| {
                let asset = AssetResolver::resolve_by_code(code)?;
                let metadata = MetadataRegistry::get_by_code(code)?;
                Some(SupportedAsset {
                    code: asset.code,
                    name: metadata.name,
                    decimals: asset.decimals,
                    icon_url: metadata.visuals.icon_url,
                })
            })
            .collect()
    }
}
```

### Pattern 4: Asset Amount Validation

```rust
use soroban_sdk::{contractimpl, String};
use crate::assets::{AssetResolver, StellarAsset};

#[contractimpl]
impl CoreContract {
    /// Validate and normalize amount based on asset decimals
    fn validate_amount(
        env: &Env,
        asset: &StellarAsset,
        amount: i128,
    ) -> Result<i128, String> {
        // Get the configured asset to verify decimals
        let configured = AssetResolver::validate(asset)
            .then_some(())
            .ok_or_else(|| String::from_str(env, "Asset not supported"))?;

        // Validate amount is positive
        if amount <= 0 {
            return Err(String::from_str(env, "Amount must be positive"));
        }

        // Calculate minimum amount based on decimals
        let min_amount = 10_i128.pow(asset.decimals);
        if amount < min_amount {
            return Err(String::from_str(env, "Amount below minimum for asset"));
        }

        Ok(amount)
    }
}
```

### Pattern 5: Multi-Asset Support

```rust
use soroban_sdk::contractimpl;
use crate::assets::AssetResolver;

#[contractimpl]
impl CoreContract {
    /// Deposit multiple assets
    pub fn batch_deposit(
        env: Env,
        deposits: Vec<(String, i128)>,
    ) -> Result<(), String> {
        for (code, amount) in deposits {
            let code_str = std::str::from_utf8(code.as_raw().as_slice())
                .map_err(|_| String::from_str(&env, "Invalid code"))?;

            // Verify asset is supported
            AssetResolver::resolve_by_code(code_str)
                .ok_or_else(|| String::from_str(&env, "Asset not supported"))?;

            // Process deposit...
        }
        Ok(())
    }
}
```

### Pattern 6: Asset Conversion

```rust
use soroban_sdk::contractimpl;
use crate::assets::PriceFeedProvider;

#[contractimpl]
impl CoreContract {
    /// Convert between assets using price feeds
    pub fn convert(
        env: Env,
        from_asset: String,
        to_asset: String,
        amount: i128,
    ) -> Result<i128, String> {
        let from_str = std::str::from_utf8(from_asset.as_raw().as_slice())
            .map_err(|_| String::from_str(&env, "Invalid source asset"))?;
        let to_str = std::str::from_utf8(to_asset.as_raw().as_slice())
            .map_err(|_| String::from_str(&env, "Invalid target asset"))?;

        PriceFeedProvider::convert(from_str, to_str, amount)
            .ok_or_else(|| String::from_str(&env, "Conversion not available"))
    }
}
```

## Storage Integration

### Example: Asset Balance Storage

```rust
use soroban_sdk::{Address, contracttype};

#[contracttype]
pub struct AssetBalance {
    pub asset_code: String,
    pub balance: i128,
}

// In contract methods:
// storage::set(&env, Key::AssetBalance(account, asset_code), &balance);
```

### Example: Asset Whitelist

```rust
// Store which assets are allowed for specific operations
fn is_asset_whitelisted(env: &Env, code: &str) -> bool {
    // Check if asset is in our supported list
    AssetResolver::is_supported(code)
}
```

## Event Integration

```rust
use soroban_sdk::{contracttype, symbol_short};

#[contracttype]
pub enum Event {
    AssetDeposited {
        asset_code: String,
        amount: i128,
        account: Address,
    },
    AssetTransferred {
        asset_code: String,
        from: Address,
        to: Address,
        amount: i128,
    },
}

// In contract methods:
// env.events().publish((symbol_short!("deposit"),), Event::AssetDeposited { ... });
```

## Testing Integration

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;
    use crate::assets::{AssetRegistry, AssetResolver};

    #[test]
    fn test_asset_transfer() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let asset = AssetRegistry::usdc();
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        // Test that transfer validates asset
        let result = client.transfer_asset(&asset, &to, &1_000_000);
        // Assert based on test expectations
    }

    #[test]
    fn test_list_supported_assets() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let assets = client.list_supported_assets();
        assert_eq!(assets.len(), 5); // 5 supported assets
    }
}
```

## Common Integration Points

### 1. Validator Functions

```rust
fn validate_transfer_asset(asset: &StellarAsset) -> bool {
    AssetValidator::validate_asset(asset).is_ok()
}
```

### 2. Lookup Functions

```rust
fn get_asset_decimals(code: &str) -> Option<u32> {
    AssetResolver::resolve_by_code(code).map(|a| a.decimals)
}
```

### 3. Display Functions

```rust
fn asset_display_name(code: &str) -> Option<String> {
    MetadataRegistry::get_by_code(code).map(|m| m.name)
}
```

### 4. Configuration Check

```rust
fn is_configured_asset(asset: &StellarAsset) -> bool {
    AssetResolver::validate(asset)
}
```

## Error Handling Examples

```rust
use soroban_sdk::String;
use crate::assets::AssetValidationError;

fn handle_asset_error(env: &Env, error: AssetValidationError) -> String {
    match error {
        AssetValidationError::UnsupportedAsset => {
            String::from_str(env, "This asset is not supported")
        }
        AssetValidationError::InvalidAssetCode => {
            String::from_str(env, "Invalid asset code format")
        }
        AssetValidationError::InvalidIssuer => {
            String::from_str(env, "Invalid issuer address")
        }
        AssetValidationError::IncorrectDecimals => {
            String::from_str(env, "Asset has incorrect decimal configuration")
        }
        _ => String::from_str(env, "Asset validation failed"),
    }
}
```

## Performance Tips

1. **Cache asset data** - Store resolved assets in local variables
2. **Batch operations** - Process multiple assets together
3. **Lazy loading** - Only resolve metadata when needed
4. **Avoid redundant validation** - Validate once, reuse result

## Security Considerations

1. **Always validate** - Validate assets from external sources
2. **Check issuers** - Verify issuer addresses match configuration
3. **Validate amounts** - Check for overflow/underflow
4. **Access control** - Ensure only authorized accounts can use assets
5. **Fail safely** - Return errors rather than panicking

## Migration Checklist

- [ ] Import asset modules in your files
- [ ] Update validators to use `AssetValidator`
- [ ] Replace hardcoded asset checks with `AssetResolver`
- [ ] Add metadata retrieval for responses
- [ ] Integrate with existing storage
- [ ] Update event schemas
- [ ] Write integration tests
- [ ] Update documentation
- [ ] Test with all 5 assets
- [ ] Review error handling

## Next Steps

1. Review the [ASSET_MANAGEMENT.md](ASSET_MANAGEMENT.md) for complete API docs
2. Check [examples/asset_management.rs](examples/asset_management.rs) for code examples
3. Look at [ASSET_REFERENCE.md](ASSET_REFERENCE.md) for quick lookups
4. Review the implementation in [crates/contracts/core/src/assets/](crates/contracts/core/src/assets/)

---

For questions or issues, refer to the comprehensive documentation included with this system.
