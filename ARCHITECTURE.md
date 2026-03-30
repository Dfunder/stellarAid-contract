# Asset Management System Architecture

## System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                 Stellar Asset Management                     │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │               Core Contract (lib.rs)                 │   │
│  │                                                      │   │
│  │  ├─ validation (Address validation)                 │   │
│  │  └─ assets (NEW - Asset management)                 │   │
│  └──────────────────────────────────────────────────────┘   │
│                              │                                │
│              ┌───────────────┼───────────────┐               │
│              ▼               ▼               ▼               │
│        ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│        │ config   │  │ metadata │  │ resolver │             │
│        │          │  │          │  │          │             │
│        │ Registry │  │ Registry │  │ Resolver │             │
│        └────▲─────┘  └────▲─────┘  └────▲─────┘             │
│             │             │             │                   │
│             │        ┌────────────┐      │                  │
│             │        │ validation │      │                  │
│             │        │ Validator  │      │                  │
│             └───┬────┴────────────┴──┬───┘                  │
│                 │                    │                      │
│                 └──────────┬─────────┘                      │
│                            │                                │
│                  ┌─────────▼────────┐                       │
│                  │  price_feeds     │                       │
│                  │  Provider        │                       │
│                  │  & Config        │                       │
│                  └──────────────────┘                       │
└─────────────────────────────────────────────────────────────┘
```

## Module Dependencies

```
mod.rs (public API)
  ├── config
  │   └── StellarAsset
  │       └── is_xlm()
  │       └── id()
  │
  ├── metadata
  │   ├── AssetMetadata
  │   ├── AssetVisuals
  │   └── MetadataRegistry
  │       ├── xlm(), usdc(), ngnt(), usdt(), eurt()
  │       └── get_by_code()
  │
  ├── resolver
  │   └── AssetResolver
  │       ├── resolve_by_code()
  │       ├── is_supported()
  │       ├── validate()
  │       └── resolve_with_metadata()
  │
  ├── validation
  │   ├── AssetValidationError
  │   └── AssetValidator
  │       ├── validate_asset()
  │       ├── verify_decimals()
  │       └── validate_complete()
  │
  └── price_feeds
      ├── PriceData
      ├── ConversionRate
      ├── PriceFeedConfig
      └── PriceFeedProvider
          ├── convert()
          ├── is_price_fresh()
          └── validate_price()
```

## Data Flow Diagram

### Asset Resolution Flow

```
┌──────────────────┐
│ Asset Code Input │
│   (e.g., "XLM")  │
└────────┬─────────┘
         │
         ▼
┌────────────────────────┐
│ AssetResolver::        │
│ resolve_by_code()      │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐      ┌──────────────────┐
│ AssetRegistry match    │─────▶│ StellarAsset     │
│ configuration          │      │ struct returned  │
└────────────────────────┘      └──────────────────┘
```

### Asset Validation Flow

```
┌──────────────────┐
│ StellarAsset     │
│ to validate      │
└────────┬─────────┘
         │
         ▼
┌────────────────────────────────┐
│ AssetValidator::               │
│ validate_complete()            │
└────────┬───────────────────────┘
         │
         ├─▶ is_valid_asset_code()
         │
         ├─▶ is_valid_issuer()
         │
         ├─▶ verify_decimals()
         │
         ├─▶ validate_asset()
         │
         ▼
┌────────────────────────────┐
│ Result                     │
│ - Ok(())                   │
│ - Err(AssetValidation...)  │
└────────────────────────────┘
```

### Asset with Metadata Lookup

```
┌──────────────────┐
│ Asset Code       │
│ "USDC"           │
└────────┬─────────┘
         │
         ├─────────────────────┬──────────────────┐
         ▼                     ▼                  ▼
    ┌─────────┐          ┌──────────┐      ┌──────────┐
    │ Asset   │          │ Metadata │      │ Visuals  │
    │Registry │          │ Registry │      │ (Icons)  │
    └────┬────┘          └────┬─────┘      └────┬─────┘
         │                    │                 │
         ├────┬───────────────┴─────────────┬───┤
         │    │                             │   │
         ▼    ▼                             ▼   ▼
    ┌─────────────────────────────────────────────┐
    │ (StellarAsset, AssetMetadata)               │
    │ with icons, logos, and metadata             │
    └─────────────────────────────────────────────┘
```

## Asset Configuration Hierarchy

```
┌──────────────────────────────┐
│ Supported Assets (5 total)   │
├──────────────────────────────┤
│                              │
│ ┌────────────────────────┐   │
│ │ XLM (Stellar Lumens)   │   │
│ ├────────────────────────┤   │
│ │ Code: XLM              │   │
│ │ Issuer: (native)       │   │
│ │ Decimals: 7            │   │
│ │ Name: Stellar Lumens   │   │
│ │ Icon: [URL]            │   │
│ │ Logo: [URL]            │   │
│ │ Color: #14B8A6         │   │
│ └────────────────────────┘   │
│                              │
│ ┌────────────────────────┐   │
│ │ USDC (Circle)          │   │
│ ├────────────────────────┤   │
│ │ Code: USDC             │   │
│ │ Issuer: GA5Z...        │   │
│ │ Decimals: 6            │   │
│ │ Name: USD Coin         │   │
│ │ Icon: [URL]            │   │
│ │ Logo: [URL]            │   │
│ │ Color: #2775CA         │   │
│ └────────────────────────┘   │
│                              │
│ ┌────────────────────────┐   │
│ │ NGNT (Nigeria)         │   │
│ ├────────────────────────┤   │
│ │ Code: NGNT             │   │
│ │ Issuer: GAUY...        │   │
│ │ Decimals: 6            │   │
│ │ Name: Nigerian Naira   │   │
│ │ Icon: [URL]            │   │
│ │ Logo: [URL]            │   │
│ │ Color: #009E73         │   │
│ └────────────────────────┘   │
│                              │
│ ┌────────────────────────┐   │
│ │ USDT (Tether)          │   │
│ ├────────────────────────┤   │
│ │ Code: USDT             │   │
│ │ Issuer: GBBD...        │   │
│ │ Decimals: 6            │   │
│ │ Name: Tether           │   │
│ │ Icon: [URL]            │   │
│ │ Logo: [URL]            │   │
│ │ Color: #26A17B         │   │
│ └────────────────────────┘   │
│                              │
│ ┌────────────────────────┐   │
│ │ EURT (Wirex)           │   │
│ ├────────────────────────┤   │
│ │ Code: EURT             │   │
│ │ Issuer: GAP5...        │   │
│ │ Decimals: 6            │   │
│ │ Name: Euro Token       │   │
│ │ Icon: [URL]            │   │
│ │ Logo: [URL]            │   │
│ │ Color: #003399         │   │
│ └────────────────────────┘   │
│                              │
└──────────────────────────────┘
```

## Type Relationships

```
┌─────────────────────────────────┐
│ AssetRegistry (config.rs)       │
│ - Static asset configurations   │
└─────────────────┬───────────────┘
                  │
                  ├─ StellarAsset
                  │   ├── code: String
                  │   ├── issuer: String
                  │   └── decimals: u32
                  │
                  └─ Returns Array[5]

┌─────────────────────────────────┐
│ MetadataRegistry (metadata.rs)  │
│ - Asset metadata & visuals      │
└─────────────────┬───────────────┘
                  │
                  ├─ AssetMetadata
                  │   ├── code: String
                  │   ├── name: String
                  │   ├── organization: String
                  │   ├── description: String
                  │   ├── visuals: AssetVisuals
                  │   └── website: String
                  │
                  ├─ AssetVisuals
                  │   ├── icon_url: String
                  │   ├── logo_url: String
                  │   └── color: String
                  │
                  └─ Returns Option<AssetMetadata>

┌─────────────────────────────────┐
│ AssetResolver (resolver.rs)     │
│ - Asset lookup & validation     │
└─────────────────┬───────────────┘
                  │
                  ├─ resolve_by_code() → Option<StellarAsset>
                  ├─ is_supported() → bool
                  ├─ validate() → bool
                  └─ resolve_with_metadata() →
                       Option<(StellarAsset, AssetMetadata)>

┌─────────────────────────────────┐
│ AssetValidator (validation.rs)  │
│ - Asset validation              │
└─────────────────┬───────────────┘
                  │
                  ├─ AssetValidationError (enum)
                  │   ├── UnsupportedAsset
                  │   ├── InvalidAssetCode
                  │   ├── InvalidIssuer
                  │   ├── IncorrectDecimals
                  │   └── MetadataMismatch
                  │
                  └─ validate_complete() →
                       Result<(), AssetValidationError>

┌─────────────────────────────────┐
│ PriceFeedProvider (price_feeds) │
│ - Price & conversion operations │
└─────────────────┬───────────────┘
                  │
                  ├─ PriceData
                  │   ├── asset_code: String
                  │   ├── price: i128
                  │   ├── decimals: u32
                  │   ├── timestamp: u64
                  │   └── source: String
                  │
                  ├─ ConversionRate
                  │   ├── from_asset: String
                  │   ├── to_asset: String
                  │   ├── rate: i128
                  │   ├── decimals: u32
                  │   └── timestamp: u64
                  │
                  ├─ PriceFeedConfig
                  │   ├── oracle_address: String
                  │   ├── fallback_oracle: String
                  │   ├── max_price_age: u64
                  │   └── use_oracle: bool
                  │
                  └─ convert() → Option<i128>
```

## Integration Points

```
┌─────────────────────────────────────────────────────────┐
│ Smart Contract Methods                                   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  transfer_asset()                                       │
│    └─ AssetValidator::validate_complete()               │
│                                                          │
│  get_asset_info()                                       │
│    └─ AssetResolver::resolve_with_metadata()            │
│                                                          │
│  list_supported_assets()                                │
│    └─ AssetResolver::supported_codes()                  │
│        └─ MetadataRegistry::get_by_code()               │
│                                                          │
│  convert_asset()                                        │
│    └─ PriceFeedProvider::convert()                      │
│                                                          │
│  validate_trust_line()                                  │
│    └─ AssetValidator methods                            │
│                                                          │
└─────────────────────────────────────────────────────────┘
         │
         ├─────────────────────────────────────────┐
         │                                          │
         ▼                                          ▼
┌──────────────────┐                    ┌──────────────────┐
│ Storage Layer    │                    │ Response/Events  │
├──────────────────┤                    ├──────────────────┤
│ Asset balances   │                    │ Asset metadata   │
│ Trust lines      │                    │ Price data       │
│ Configurations   │                    │ Conversion rates │
└──────────────────┘                    └──────────────────┘
```

## Performance Characteristics

```
Operation                  Time    Space   Notes
─────────────────────────────────────────────────────
resolve_by_code()         O(1)    O(1)    Direct match
is_supported()            O(1)    O(1)    Simple comparison
validate_asset()          O(1)    O(1)    Fixed checks
get_metadata()            O(1)    O(1)    Hash lookup
convert_amount()          O(1)    O(1)    Single multiplication
list_all_assets()         O(5)    O(5)    Fixed 5 assets
validate_complete()       O(1)    O(1)    All checks O(1)
```

## Security Model

```
┌─────────────────────────────────┐
│ User Input                      │
│ (asset code, issuer, amount)    │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ Validation Layer                │
├─────────────────────────────────┤
│ • Code format check             │
│ • Issuer address validation     │
│ • Decimal verification          │
│ • Type safety                   │
│ • Bounds checking               │
│ • Error handling (no panic)    │
└────────────┬────────────────────┘
             │
             ▼ (Safe or Error)
┌─────────────────────────────────┐
│ Execution Layer                 │
│ (Safe to proceed)               │
└─────────────────────────────────┘
```

## Extension Model

```
┌──────────────────────────────────────────┐
│ How to Add New Assets                    │
├──────────────────────────────────────────┤
│                                          │
│ 1. config.rs                             │
│    └─ Add to AssetRegistry               │
│       └─ Add to all_assets()             │
│       └─ Add to all_codes()              │
│                                          │
│ 2. metadata.rs                           │
│    └─ Add to MetadataRegistry            │
│       └─ Add to get_by_code()            │
│       └─ Add to all()                    │
│                                          │
│ 3. resolver.rs                           │
│    └─ Update resolve_by_code()           │
│    └─ Update is_supported()              │
│                                          │
│ 4. validation.rs                         │
│    └─ Update verify_decimals()           │
│                                          │
│ 5. Tests & Updates                       │
│    └─ Add unit tests                     │
│    └─ Update JSON config                │
│    └─ Update documentation               │
│                                          │
└──────────────────────────────────────────┘
```

## File Organization

```
crates/contracts/core/src/
├── lib.rs (exports assets module)
└── assets/
    ├── mod.rs (module aggregation)
    ├── config.rs (asset definitions)
    ├── metadata.rs (metadata + icons)
    ├── resolver.rs (lookup utilities)
    ├── validation.rs (validation logic)
    └── price_feeds.rs (price integration)

Documentation/
├── ASSET_MANAGEMENT.md (complete API)
├── ASSET_REFERENCE.md (quick reference)
├── ASSET_INTEGRATION_GUIDE.md (patterns)
├── README_ASSETS.md (overview)
├── IMPLEMENTATION_SUMMARY.md (what built)
└── VERIFICATION_CHECKLIST.md (validation)

Configuration/
├── assets-config.json (JSON config)
└── examples/asset_management.rs (examples)
```

---

This architecture provides:
- ✅ Type-safe asset operations
- ✅ O(1) resolution and validation
- ✅ Comprehensive error handling
- ✅ Clear extension points
- ✅ Security at every layer

---

## Testing Architecture

### Comprehensive Test Suite

The StellarAid contract implements a multi-layered testing strategy:

```
┌─────────────────────────────────────────────────────────────┐
│                 Testing Architecture                        │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │            Integration Tests (50+)                  │   │
│  │  ┌─────────────────────────────────────────────────┐ │   │
│  │  │  Unit Tests (lib.rs + modules)                 │ │   │
│  │  │  ┌────────────────────────────────────────────┐ │ │   │
│  │  │  │  Performance Tests (storage_tests.rs)     │ │ │   │
│  │  │  └────────────────────────────────────────────┘ │ │   │
│  │  └─────────────────────────────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────┘   │
│           │                                                 │
│           ▼                                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │            CI/CD Pipeline                           │   │
│  │  ┌─────────────────────────────────────────────────┐ │   │
│  │  │  Coverage Reports (tarpaulin)                 │ │   │
│  │  │  ┌────────────────────────────────────────────┐ │ │   │
│  │  │  │  Security Audit (cargo-audit)            │ │ │   │
│  │  │  └────────────────────────────────────────────┘ │ │   │
│  │  └─────────────────────────────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Test Categories

#### 1. Unit Tests (`src/lib.rs`, `src/*/tests.rs`)
- Individual function validation
- Module isolation testing
- Error condition handling
- Edge case validation

#### 2. Integration Tests (`src/integration_tests.rs`)
- **50+ comprehensive tests** covering:
  - Real-world donation flows
  - Multi-user scenarios
  - Asset management workflows
  - Security boundary validation
  - Performance under load
  - Edge cases and error conditions

#### 3. Performance Tests (`src/storage_tests.rs`)
- Storage optimization validation
- Gas efficiency measurement
- Scalability testing
- Read/write pattern analysis

### Test Execution Strategy

#### Local Development
```bash
make test              # All tests
make test-unit         # Unit tests only
make test-integration  # Integration tests only
make test-coverage     # With coverage report
make performance-test  # Performance validation
```

#### CI/CD Pipeline
- Automated testing on push/PR
- Coverage reporting (>80% target)
- Security vulnerability scanning
- Performance regression detection
- 2-minute execution time limit

### Test Organization

#### File Structure
```
src/
├── lib.rs                    # Unit tests for core functions
├── integration_tests.rs      # Comprehensive integration suite
├── storage_tests.rs          # Performance and storage tests
├── validation/
│   └── tests.rs             # Address validation tests
└── assets/
    └── [module]_tests.rs    # Asset management tests
```

#### Test Naming Convention
- `test_[category]_[scenario]_[condition]`
- Examples:
  - `test_donation_basic_flow`
  - `test_duplicate_transaction_rejection`
  - `test_admin_withdrawal_insufficient_balance`
  - `test_large_number_of_donations`

### Coverage Requirements

#### Functional Coverage
- ✅ Contract initialization and setup
- ✅ Donation creation and validation
- ✅ Asset management (add/remove/list)
- ✅ Withdrawal operations
- ✅ Admin functions and RBAC
- ✅ Event emission and logging
- ✅ Error handling and edge cases

#### Security Coverage
- ✅ Access control validation
- ✅ Input sanitization
- ✅ Reentrancy protection
- ✅ Transaction deduplication
- ✅ Asset validation and verification

#### Performance Coverage
- ✅ Storage efficiency (hashing, symbols)
- ✅ Gas optimization
- ✅ Scalability (100-1000 donations)
- ✅ Concurrent operation handling

### Test Data Management

#### Deterministic Test Data
- Generated addresses for each test
- Unique transaction hashes
- Varied amounts and assets
- Edge case inputs (empty, long, unicode)

#### State Isolation
- Fresh contract instance per test
- Clean storage state
- Independent token contracts
- No cross-test interference

### Validation Framework

#### Assertions
- Return value validation
- Event emission verification
- State change confirmation
- Error condition handling
- Performance metric checking

#### Test Helpers
- `setup_contract()` - Contract initialization
- `setup_token()` - Token contract creation
- `create_test_donation_data()` - Standard test data
- Time-based validation
- Balance verification

### CI/CD Integration

#### Automated Checks
- Code formatting (`cargo fmt`)
- Linting (`cargo clippy`)
- Security audit (`cargo audit`)
- Dependency verification (`cargo deny`)
- Test execution with coverage
- Performance benchmarking

#### Quality Gates
- All tests pass
- Coverage > 80%
- No security vulnerabilities
- Performance within limits
- Clean code standards

### Maintenance Guidelines

#### Adding New Tests
1. Identify appropriate test category
2. Follow naming conventions
3. Include comprehensive assertions
4. Document edge cases covered
5. Update coverage metrics

#### Test Data Updates
- Keep test data realistic
- Include boundary conditions
- Document data generation patterns
- Ensure deterministic behavior

#### Performance Monitoring
- Track test execution time
- Monitor gas usage patterns
- Validate scalability assumptions
- Update benchmarks as needed
