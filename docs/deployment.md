# Contract Deployment Guide

This guide covers deploying StellarAid contracts to testnet and mainnet step by step.

---

## Prerequisites

- Rust stable toolchain (managed by `rust-toolchain.toml`)
- `wasm32v1-none` target: `rustup target add wasm32v1-none`
- Stellar CLI: `cargo install stellar-cli --locked`
- Funded Stellar account (testnet: use [Friendbot](https://friendbot.stellar.org))

---

## Environment Variables

Copy `.env.example` to `.env` and fill in the required values:

```bash
cp .env.example .env
```

| Variable | Required | Description |
|---|---|---|
| `SOROBAN_NETWORK` | Yes | `testnet` or `mainnet` |
| `SOROBAN_ADMIN_SECRET_KEY` | Yes | Secret key of the deploying account |
| `SOROBAN_ADMIN_PUBLIC_KEY` | Yes | Corresponding public key |
| `SOROBAN_TESTNET_RPC_URL` | Testnet | Soroban RPC endpoint for testnet |
| `SOROBAN_TESTNET_PASSPHRASE` | Testnet | `Test SDF Network ; September 2015` |
| `SOROBAN_MAINNET_RPC_URL` | Mainnet | Soroban RPC endpoint for mainnet |
| `SOROBAN_MAINNET_PASSPHRASE` | Mainnet | `Public Global Stellar Network ; September 2015` |
| `ASSET_CODE` | Optional | Custom asset code (default: `STAID`) |

> **Never commit your `.env` file or secret keys to version control.**

---

## Build

```bash
# Build all contracts as WASM
cargo build --target wasm32v1-none --release --all

# Verify WASM artifacts
find target/wasm32v1-none/release -maxdepth 1 -name "*.wasm"
```

---

## Testnet Deployment

### 1. Configure network

```bash
stellar network add testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"
```

### 2. Import your account

```bash
stellar keys add deployer --secret-key $SOROBAN_ADMIN_SECRET_KEY
```

### 3. Fund the account (testnet only)

```bash
stellar keys fund deployer --network testnet
```

### 4. Deploy the campaign contract

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/campaign.wasm \
  --source deployer \
  --network testnet
```

The command outputs a **Contract ID** â€” save it for initialization.

### 5. Initialize the contract

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin $SOROBAN_ADMIN_PUBLIC_KEY
```

### 6. Verify deployment

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- get_config
```

---

## Mainnet Deployment

> **Warning:** Mainnet transactions are irreversible and cost real XLM. Test thoroughly on testnet first.

### 1. Configure network

```bash
stellar network add mainnet \
  --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015"
```

### 2. Import your account

```bash
stellar keys add deployer --secret-key $SOROBAN_ADMIN_SECRET_KEY
```

### 3. Ensure the account is funded

Your account must hold sufficient XLM to cover transaction fees and contract storage rent. Check balance:

```bash
stellar account show $SOROBAN_ADMIN_PUBLIC_KEY --network mainnet
```

### 4. Deploy

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/campaign.wasm \
  --source deployer \
  --network mainnet
```

### 5. Initialize

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source deployer \
  --network mainnet \
  -- initialize \
  --admin $SOROBAN_ADMIN_PUBLIC_KEY
```

---

## Contract Initialization

After deployment, initialize each contract with its required parameters:

### campaign

```bash
stellar contract invoke \
  --id <CAMPAIGN_CONTRACT_ID> \
  --source deployer \
  --network <NETWORK> \
  -- initialize \
  --admin <ADMIN_ADDRESS>
```

### token-bridge

```bash
stellar contract invoke \
  --id <TOKEN_BRIDGE_CONTRACT_ID> \
  --source deployer \
  --network <NETWORK> \
  -- initialize \
  --admin <ADMIN_ADDRESS>
```

---

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `WASM file not found` | Contract not built | Run `cargo build --target wasm32v1-none --release --all` |
| `insufficient balance` | Account not funded | Fund via Friendbot (testnet) or transfer XLM (mainnet) |
| `stellar: command not found` | CLI not installed | Run `cargo install stellar-cli --locked` |
| `invalid network passphrase` | Wrong passphrase in config | Verify passphrase matches the network exactly |
| `contract already initialized` | `initialize` called twice | Contract state is already set; skip initialization |
| `transaction simulation failed` | RPC endpoint unreachable | Check `SOROBAN_TESTNET_RPC_URL` / `SOROBAN_MAINNET_RPC_URL` |
| `account not found` | Key not imported | Run `stellar keys add deployer --secret-key <KEY>` |