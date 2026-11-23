# Multi-Wallet Session Contract

`multi_wallet_session` manages switching and session state isolation for multi-wallet operations in StellarAid / Lumora.

## Status

- **Status**: Work in Progress / Experimental.
- **Manifest**: Restored and standardized with Soroban SDK 21 and workspace conventions (closes #1145).
- **Logic & Security**: Logic defects, session validation hardening, and storage TTL policies are tracked in separate dedicated issues.
- **Compilation**: Passes `cargo check -p multi_wallet_session --target wasm32-unknown-unknown`.

## Build

```bash
cargo check -p multi_wallet_session --target wasm32-unknown-unknown
cargo build -p multi_wallet_session --target wasm32-unknown-unknown --release
```
