# Contract versioning

`contracts/shared/src/version.rs` is the single semantic-version implementation
for this workspace. `contracts/shared/src/upgrade.rs` owns the stored
`ContractVersion` value and the `UpgradeKey::Version` storage key. The former
`contracts/semver_types.rs` include file has been deleted; no crate splices a
second version implementation into its source.

Every contract that exposes version queries has three thin `#[contractimpl]`
methods that delegate to `shared::version::query`, `query_metadata`, and
`ContractVersion::is_compatible_with`. The wrappers return
`shared::upgrade::ContractVersion` and `shared::version::VersionMetadata`, so
all crates agree on types and ABI while keeping the generated client surface
stable. `shared::impl_semver_queries!()` remains available for non-contract
helpers but is not used inside `#[contractimpl]` because the contract macro
does not expand nested macro calls when generating clients.

`initialize` calls `shared::version::seed(env, env!("CARGO_PKG_VERSION"))` after
its admin/auth checks. Queries read the seeded instance-storage value and fall
back to the WASM crate version only if seeding has not happened yet.
`shared::upgrade::record_upgrade` replaces the stored value on upgrade.

Compatibility rules live in `ContractVersion::is_compatible_with`:

- the major version must match;
- `0.x` requires the same minor version and a patch greater than or equal to
  the caller's requirement;
- `>= 1.0` accepts any newer minor/patch of the same major, but rejects an
  older version than the caller requires.

Build-time `CARGO_PKG_NAME` and `CARGO_PKG_VERSION` are Cargo variables, not
operator environment variables. They are consumed by the macro and are
documented in [ENV_REFERENCE.md](./ENV_REFERENCE.md) for completeness.
