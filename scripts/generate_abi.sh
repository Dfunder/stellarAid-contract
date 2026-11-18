#!/usr/bin/env bash
set -euo pipefail

# generate_abi.sh — Generate Soroban ABI (contract spec) JSON for built contract WASM artifacts.
#
# Usage:
#   ./scripts/generate_abi.sh [output_dir] [wasm_dir]
#
# Defaults:
#   output_dir = ./abis
#   wasm_dir   = ${WASM_DIR:-./target/wasm32-unknown-unknown/release}

OUTPUT_DIR="${1:-./abis}"
WASM_DIR="${WASM_DIR:-${2:-./target/wasm32-unknown-unknown/release}}"

if ! command -v soroban &> /dev/null; then
    echo "Error: 'soroban' CLI not found. Install it with:" >&2
    echo "  cargo install soroban-cli" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

shopt -s nullglob
wasms=( "$WASM_DIR"/*.wasm )

if [ ${#wasms[@]} -eq 0 ]; then
    echo "Error: no WASM artifacts found in $WASM_DIR" >&2
    echo "Run a release wasm build first (e.g. cargo build --workspace --target wasm32-unknown-unknown --release)." >&2
    exit 1
fi

echo "Generating ABIs from: $WASM_DIR"
echo "Output directory: $OUTPUT_DIR"

count=0
for wasm_path in "${wasms[@]}"; do
    contract_name="$(basename "$wasm_path" .wasm)"
    out="$OUTPUT_DIR/${contract_name}.json"
    echo "  Generating ABI for: $contract_name"
    soroban contract spec --wasm "$wasm_path" --output "$out"
    count=$((count + 1))
done

echo "Done. Generated $count ABI file(s) in $OUTPUT_DIR/"
