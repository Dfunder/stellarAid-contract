# Contract Deployment Guide

## Prerequisites

- Rust + `rustup` (stable toolchain)
- `stellar-cli`: `cargo install --locked stellar-cli --features opt`
- WASM target: `rustup target add wasm32v1-none`
- A funded Stellar account (source keypair)

## Environment Variables

| Variable | Description |
|---|---|
| `STELLAR_SECRET_KEY` | Deployer keypair secret |
| `STELLAR_NETWORK` | `testnet` or `mainnet` |
| `STELLAR_RPC_URL` | Soroban RPC endpoint |

## Testnet Deployment

```bash
make setup && make build
stellar contract deploy   --wasm target/wasm32v1-none/release/campaign.wasm   --source $STELLAR_SECRET_KEY   --network testnet
```

## Mainnet Deployment

```bash
stellar contract deploy   --wasm target/wasm32v1-none/release/campaign.wasm   --source $STELLAR_SECRET_KEY   --network mainnet   --rpc-url $STELLAR_RPC_URL
```

## Contract Initialization

```bash
stellar contract invoke   --id $CONTRACT_ID   --source $STELLAR_SECRET_KEY   --network testnet   -- initialize
```

## Troubleshooting

- **`InsufficientFee`**: Add `--fee 1000000` to the deploy command.
- **`WasmAlreadyExists`**: Binary is already on-chain; proceed directly to `invoke`.
- **WASM target missing**: Run `rustup target add wasm32v1-none`.
