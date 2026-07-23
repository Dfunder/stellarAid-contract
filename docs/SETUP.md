# Setup Guide

## Prerequisites

### Install Rust with wasm32 target

1. Install Rust via rustup:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Add the `wasm32-unknown-unknown` target required for compiling Soroban contracts:

   ```bash
   rustup target add wasm32-unknown-unknown
   ```

### Install Soroban CLI

Install the Soroban CLI using Cargo with the `--locked` flag to ensure reproducible builds:

```bash
cargo install --locked soroban-cli
```

> **Note:** This project uses `soroban-sdk` version **21.7.0**. Ensure your Soroban CLI version is compatible.

## Configure Testnet Network

Add the Stellar testnet to your Soroban CLI configuration:

```bash
soroban network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"
```

Verify the network was added:

```bash
soroban network ls
```

## Environment Variables

Copy `.env.example` to `.env` and fill in the required values:

```bash
cp .env.example .env
```

| Variable                    | Description                                     |
|-----------------------------|-------------------------------------------------|
| `STELLAR_NETWORK`           | Network to use (`testnet` or `mainnet`)         |
| `STELLAR_PLATFORM_SECRET`   | Stellar secret key for the platform account     |
| `HORIZON_URL`               | Horizon API endpoint URL                        |
| `SOROBAN_RPC_URL`           | Soroban RPC endpoint URL                        |
| `SOROBAN_NETWORK_PASSPHRASE`| Network passphrase for signing transactions     |

## SDK Version Note

This project targets **soroban-sdk 21.7.0**. When adding new contracts or dependencies, ensure they reference the workspace dependency:

```toml
[dependencies]
soroban-sdk = { workspace = true }
```
