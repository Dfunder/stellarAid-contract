# Contract Versioning Strategy

> Closes **#682** — semantic versioning, version metadata, on-chain querying, and compatibility constraints for every Lumora / StellarAid contract.

Related: [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md), [MAINTENANCE_WINDOWS.md](./MAINTENANCE_WINDOWS.md), [CHANGELOG.md](../CHANGELOG.md).

## Scheme

All contracts follow **[Semantic Versioning 2.0.0](https://semver.org/)**: `MAJOR.MINOR.PATCH`.

The crate version in each `contracts/*/Cargo.toml` (`package.version`, currently `0.1.0`) **is** the contract version. On-chain queries read the same number that was compiled into the WASM.

| Component | When to bump | Example |
|-----------|----------------|---------|
| **MAJOR** | Breaking ABI, storage-key change, error-code reuse, or removed entry point | `1.4.2` → `2.0.0` |
| **MINOR** | Backward-compatible feature (new optional field, new entry point) | `1.4.2` → `1.5.0` |
| **PATCH** | Bug fix, docs, or gas/TTL tweak with no ABI change | `1.4.2` → `1.4.3` |

### Pre-1.0 (`0.x`) exception

Until the first `1.0.0` release, a **MINOR** bump may be breaking. Clients must match `0.MINOR` exactly; only `PATCH` may differ. This matches semver's `0.y.z` stability rule.

### Storage schema (independent counter)

`shared::version::CURRENT_STORAGE_SCHEMA` (also recorded in `[package.metadata.stellar-aid] storage-schema`) is incremented when a `#[contracttype]` layout change needs a migration function. It is **not** reset when MAJOR bumps. In-place WASM replacement with a schema mismatch is forbidden without a `migrate_vN_to_vM` entry point.

## Version metadata

Every contract crate declares:

```toml
[package]
version = "0.1.0"

[package.metadata.stellar-aid]
versioning = "semver"
storage-schema = 1
min-compatible = "0.1.0"
```

Workspace defaults live in the root `Cargo.toml` under `[workspace.metadata.stellar-aid]`.

On-chain, crates that depend on `shared` call `shared::version::seed` from `initialize`, which writes a `ContractVersion` to instance storage (same key as `shared::upgrade`). After an upgrade, `shared::upgrade::record_upgrade` overwrites that value. Every contract also compiles the crate version from `Cargo.toml` into `get_version`, so a freshly deployed WASM can be queried before `initialize`.

## Version querying

Every contract exposes the same three read-only entry points (no auth):

```text
get_version()            -> ContractVersion { major, minor, patch }
get_version_metadata()   -> VersionMetadata { name, version, min_compatible, storage_schema }
is_version_compatible(major, minor, patch) -> bool
```

### CLI

```bash
# Semantic version of a live contract
soroban contract invoke \
  --id $CONTRACT_ID --network $NETWORK --source $IDENTITY -- \
  get_version

# Full metadata (crate name, min-compatible client, storage schema)
soroban contract invoke \
  --id $CONTRACT_ID --network $NETWORK --source $IDENTITY -- \
  get_version_metadata

# Can a client built against 0.1.0 talk to this WASM?
soroban contract invoke \
  --id $CONTRACT_ID --network $NETWORK --source $IDENTITY -- \
  is_version_compatible --major 0 --minor 1 --patch 0
```

If instance storage has not been seeded (contract deployed but `initialize` not yet called), `get_version` falls back to the WASM crate version compiled from `Cargo.toml`.

## Version constraints

### Client ↔ contract

| Running contract | Client required | Compatible? |
|------------------|-----------------|-------------|
| `0.1.4` | `0.1.0` | Yes (same 0.x minor, newer patch) |
| `0.1.4` | `0.1.5` | No (client newer than contract) |
| `0.1.4` | `0.2.0` | No (0.x minor mismatch is breaking) |
| `1.3.1` | `1.0.0` | Yes (same major, contract newer) |
| `1.3.1` | `1.4.0` | No (client requires a newer minor) |
| `1.3.1` | `2.0.0` | No (major mismatch) |

`VersionMetadata.min_compatible` is the lowest client version the running WASM still supports (`0.MINOR.0` while pre-1.0, `MAJOR.0.0` afterwards).

### Cross-contract calls

A caller **must** treat a callee as compatible only when `callee.is_version_compatible(expected_major, expected_minor, expected_patch)` is true for the version the caller was compiled against. Do not invoke a contract whose major (or, on `0.x`, minor) differs from the version used in integration tests.

### Upgrade constraints

1. **PATCH** — in-place WASM replace is allowed after pause; no migration.
2. **MINOR** (`>= 1.0`) — in-place replace allowed if storage schema is unchanged; new fields must be `Option<T>`.
3. **MAJOR** or **storage-schema bump** — deploy to a **new** contract ID, run the versioned `migrate_vN_to_vM` entry point, cut traffic, keep the old ID for 30 days. See [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md).
4. Never remove or rename a `#[contracttype]` / `#[contracterror]` variant; never change a `DataKey` discriminant.

### SDK / worker constraint

Off-chain services pin the contract set they talk to:

```text
required = { major: 0, minor: 1, patch: 0 }
accepted = contract.is_version_compatible(required)
```

Refuse to submit transactions when `accepted` is false. Log `get_version_metadata` in health checks.

## Documenting version changes

1. Bump `package.version` in the affected `contracts/*/Cargo.toml` (and `workspace.package.version` when all crates move together).
2. Add a section to [CHANGELOG.md](../CHANGELOG.md) under `Added` / `Changed` / `Fixed` / `Breaking`.
3. If storage layout changed, increment `storage-schema` in `[package.metadata.stellar-aid]` **and** `CURRENT_STORAGE_SCHEMA` in `contracts/shared/src/version.rs`.
4. Mention the new version in the PR title (`feat(escrow): 0.2.0 — add batch refund`).

## Current versions

| Crate | Semver | Storage schema | Min compatible client |
|-------|--------|----------------|------------------------|
| `shared` | 0.1.0 | 1 | 0.1.0 |
| `platform_config` | 0.1.0 | 1 | 0.1.0 |
| `escrow` | 0.1.0 | 1 | 0.1.0 |
| `commission_agreement` | 0.1.0 | 1 | 0.1.0 |
| `dispute_arbiter` | 0.1.0 | 1 | 0.1.0 |
| `messaging` | 0.1.0 | 1 | 0.1.0 |
| `subscription` | 0.1.0 | 1 | 0.1.0 |
| `competitions` | 0.1.0 | 1 | 0.1.0 |
| `verification` | 0.1.0 | 1 | 0.1.0 |
| `revenue_sharing` | 0.1.0 | 1 | 0.1.0 |
| `recruitment` | 0.1.0 | 1 | 0.1.0 |
| `creator_fund` | 0.1.0 | 1 | 0.1.0 |
| `campaign` | 0.1.0 | 1 | 0.1.0 |
| `donation` | 0.1.0 | 1 | 0.1.0 |
| `withdrawal` | 0.1.0 | 1 | 0.1.0 |

## Implementation map

| Piece | Location |
|-------|----------|
| Semver types + constraints | `contracts/shared/src/version.rs` |
| On-chain store / upgrade event | `contracts/shared/src/upgrade.rs` |
| Query macros (crates that use `shared`) | `shared::impl_semver_queries!()` |
| Query includes (other contracts) | `contracts/semver_types.rs` |
| Crate metadata | `contracts/*/Cargo.toml` |
| Off-chain registry helper | `contracts/version_tracking.rs` |
