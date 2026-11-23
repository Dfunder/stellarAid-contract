#!/usr/bin/env bash
set -e

NETWORK="testnet"

echo "Generating deployer keypair..."
soroban keys generate deployer --network $NETWORK
soroban keys fund deployer --network $NETWORK

echo "Building contracts..."
cargo build --target wasm32-unknown-unknown --release

echo "Deploying platform_config contract..."
PLATFORM_CONFIG_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/platform_config.wasm \
  --source deployer \
  --network $NETWORK)

echo "Deploying escrow contract..."
ESCROW_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source deployer \
  --network $NETWORK)

echo "PLATFORM_CONFIG_ID=$PLATFORM_CONFIG_ID"
echo "ESCROW_ID=$ESCROW_ID"

cat > .env.contracts <<EOF
PLATFORM_CONFIG_ID=$PLATFORM_CONFIG_ID
ESCROW_ID=$ESCROW_ID
EOF

echo "Contract addresses saved to .env.contracts"
