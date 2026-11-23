# Workspace status (compile / dependency issues)

This checkout reflects the current workspace state on `main` and documents the
status of previously reported compile/dependency issues relative to files and
crates present today.

## Crates/files not present in this checkout

The following paths referenced by open issues are not present on disk and are
not registered Rust workspace members in this checkout:

- `contracts/platform_config/src/lib.rs` and `contracts/platform_config/` (#912)
- `contracts/campaign/src/lib.rs` and `contracts/campaign/` (#913)
- `contracts/commission_agreement/src/lib.rs` and `contracts/commission_agreement/` (#914, #915)
- `contracts/dao/src/lib.rs` and `contracts/dao/` (#917)
- `contracts/donation/`, `contracts/withdrawal/`, `contracts/factory/`,
  `contracts/campaign_instance/` (not present; membership/notes reconciled in #918)

Because these crate sources are absent, the specific parse/import/symbol/API
errors described in #912, #913, #914, #915, and #917 are not reproducible in
this checkout. Workspace membership and notes have been reconciled to match the
crates actually present on disk (`Cargo.toml`, `docs/architecture.md`).

## SDK / dependency alignment

- Root `Cargo.toml` pins `stellar-strkey = "0.0.8"` (aligned with the API
  expected by Soroban/XDR dependencies) and includes a note describing the
  prior 0.0.2 pin issue (#916 context).
- The SDK Rust source paths referenced by #916 (e.g. `sdk/src/utils/keypair.rs`,
  `sdk/src/utils/xdr_parser.rs`, `sdk/src/transaction_builder.rs`,
  `sdk/src/setup/token_setup.rs`) are not present in this checkout; current
  `sdk/` contents are TypeScript/utility files.

## Current registered contract set

Registered, present contract crates: `contracts/shared`, `contracts/escrow`,
`contracts/verification`, `contracts/multi_sig`, `contracts/campaign_factory`.
All have `Cargo.toml` and `[package.metadata.stellar-aid]` metadata.
