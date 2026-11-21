# Token interface setup (escrow)

## Current approach

`contracts/escrow/src/token.rs` re-exports the token interface from
`soroban_sdk::token` (`Client`, `StellarAssetClient`, `TokenClient`, `Interface`)
instead of using `soroban_sdk::contractimport!` against a prebuilt
`soroban_token_contract.wasm` in `target/`.

This keeps `cargo check -p escrow` hermetic for a clean checkout (no manual
release WASM build step required) and avoids depending on build artifacts under
`target/`.

## Provenance / version

- Soroban SDK: `soroban-sdk = "21.0.0"` (workspace dependency, see root
  `Cargo.toml`)
- Token interface: provided by `soroban_sdk::token` in the pinned SDK version.

If a future change requires importing a specific token contract WASM artifact
(e.g. for `contractimport!` parity with a deployed token contract), commit the
WASM at a stable path in the repository (not under `target/`), record its exact
source/build command and SDK/toolchain version here, and update the import path
accordingly.
