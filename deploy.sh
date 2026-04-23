#!/usr/bin/env bash
# deploy.sh - Deploy StellarAid core contract and store the contract ID
# Closes #96

set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
WASM_PATH="target/wasm32-unknown-unknown/release/stellaraid_core.wasm"
CONTRACT_ID_FILE=".stellaraid_contract_id"

echo "🔨 Building contract for network: $NETWORK"
cargo build -p stellaraid-core --target wasm32-unknown-unknown --release

if [ ! -f "$WASM_PATH" ]; then
  echo "❌ WASM file not found at $WASM_PATH"
  exit 1
fi

echo "🚀 Deploying to network: $NETWORK"
CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --network "$NETWORK" \
  --source "${SOROBAN_ACCOUNT:-default}")

echo "✅ Contract deployed successfully!"
echo "📝 Contract ID: $CONTRACT_ID"

echo "$CONTRACT_ID" > "$CONTRACT_ID_FILE"
echo "✅ Contract ID stored in $CONTRACT_ID_FILE"
