# Wallet Sign Timeout Contract

`wallet_sign_timeout` implements ledger-based signature execution timeouts, allowing intent pre-authorization and clean retry loops if transactions expire before being signed.

## Status

- **Status**: Work in Progress / Experimental.
- **Manifest**: Restored and standardized with Soroban SDK 21 and workspace conventions (closes #1145).
- **Logic & Security**: Logic defects and deadline evaluation rules are tracked in separate dedicated issues.
- **Compilation**: Passes `cargo check -p wallet_sign_timeout --target wasm32-unknown-unknown`.

## Build

```bash
cargo check -p wallet_sign_timeout --target wasm32-unknown-unknown
cargo build -p wallet_sign_timeout --target wasm32-unknown-unknown --release
```
