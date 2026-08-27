# Lumora Contracts

## Overview

| Contract | Description |
|---|---|
| `platform_config` | Stores platform admin, fee basis points, platform wallet, and USDC token address |
| `escrow` | Manages commission escrow lifecycle: create, release, refund, dispute, expire |
| `shared` | Shared types used across contracts |

## Versioning

All crates follow semantic versioning. Query a live contract with `get_version` / `get_version_metadata`. See [../docs/VERSIONING.md](../docs/VERSIONING.md).

## Architecture

```
Client
  |
  v
escrow contract
  |-- cross-contract call -->
  platform_config contract
  |                          |
  v                          v
 USDC token transfer    reads fee_bps
```

## Prerequisites

- Rust stable toolchain
- `rustup target add wasm32-unknown-unknown`
- `cargo install --locked soroban-cli --features opt`

## Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

## Test

```bash
cargo test
```

## Deploy

See [../scripts/deploy_testnet.sh](../scripts/deploy_testnet.sh)

## Contract Addresses (Testnet)

| Contract | Address |
|---|---|
| platform_config | TBD |
| escrow | TBD |
