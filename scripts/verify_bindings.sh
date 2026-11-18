#!/usr/bin/env bash
set -euo pipefail

# verify_bindings.sh — Verify that checked-in binding specs match compiled contract WASM specs.
#
# Usage:
#   ./scripts/verify_bindings.sh [binding_dir] [wasm_dir]
#
# Defaults:
#   binding_dir = ./sdk/bindings
#   wasm_dir    = ${WASM_DIR:-./target/wasm32-unknown-unknown/release}

BINDING_DIR="${1:-./sdk/bindings}"
WASM_DIR="${WASM_DIR:-${2:-./target/wasm32-unknown-unknown/release}}"

if ! command -v soroban &> /dev/null; then
    echo "Error: 'soroban' CLI not found." >&2
    exit 1
fi

shopt -s nullglob
wasms=( "$WASM_DIR"/*.wasm )

if [ ${#wasms[@]} -eq 0 ]; then
    echo "Error: no WASM artifacts found in $WASM_DIR" >&2
    echo "Run a release wasm build first before verifying bindings." >&2
    exit 1
fi

echo "Verifying bindings against compiled contracts..."
echo "  WASM dir:  $WASM_DIR"
echo "  Bindings:  $BINDING_DIR"

tmp_spec=""
cleanup() {
    if [ -n "$tmp_spec" ] && [ -f "$tmp_spec" ]; then
        rm -f "$tmp_spec"
    fi
}
trap cleanup EXIT

EXIT_CODE=0
for contract_wasm in "${wasms[@]}"; do
    contract_name="$(basename "$contract_wasm" .wasm)"
    spec_file="$BINDING_DIR/${contract_name}.json"

    if [ ! -f "$spec_file" ]; then
        echo "  MISSING: $spec_file (no binding spec found for $contract_name)" >&2
        EXIT_CODE=1
        continue
    fi

    tmp_spec="$(mktemp)"
    if ! soroban contract spec --wasm "$contract_wasm" --output "$tmp_spec"; then
        echo "  ERROR: failed to generate spec for $contract_name" >&2
        EXIT_CODE=1
        cleanup
        tmp_spec=""
        continue
    fi

    if diff -q "$tmp_spec" "$spec_file" >/dev/null 2>&1; then
        echo "  OK: $contract_name"
    else
        echo "  MISMATCH: $contract_name (bindings are stale)" >&2
        EXIT_CODE=1
    fi

    cleanup
    tmp_spec=""
done

if [ "$EXIT_CODE" -eq 0 ]; then
    echo "All bindings are up to date."
else
    echo "Some bindings need updating or generation failed." >&2
fi

exit "$EXIT_CODE"
