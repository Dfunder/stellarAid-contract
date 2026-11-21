#!/usr/bin/env bash
# Snapshot contract versions, WASM, and config before a maintenance window.
# See docs/MAINTENANCE_WINDOWS.md (closes #681).
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
OUT="${OUT:-backups/$(date -u +%Y-%m-%dT%H%MZ)}"
CONTRACTS_FILE="${CONTRACTS_FILE:-config/testnet_contracts.json}"

mkdir -p "$OUT/wasm" "$OUT/config"

if [[ -f "$CONTRACTS_FILE" ]]; then
  cp "$CONTRACTS_FILE" "$OUT/config/"
fi

# CONTRACTS is a space-separated list of "name:id" pairs, e.g.
#   CONTRACTS="escrow:CABC... platform_config:CDEF..." ./scripts/backup_contract_state.sh
if [[ -z "${CONTRACTS:-}" ]]; then
  echo "Set CONTRACTS='name:id name:id' (and optional NETWORK, CONTRACTS_FILE)."
  echo "Wrote empty inventory dir at $OUT"
  echo "{\"network\":\"$NETWORK\",\"created\":\"$(date -u +%Y-%m-%dT%H:%MZ)\"}" > "$OUT/inventory.json"
  exit 0
fi

{
  echo "{"
  echo "  \"network\": \"$NETWORK\","
  echo "  \"created\": \"$(date -u +%Y-%m-%dT%H:%MZ)\","
  echo "  \"contracts\": ["
  first=1
  for entry in $CONTRACTS; do
    name="${entry%%:*}"
    id="${entry#*:}"
    wasm="target/wasm32-unknown-unknown/release/${name}.wasm"

    if [[ "$first" -eq 1 ]]; then first=0; else echo ","; fi
    printf '    {"name":"%s","id":"%s"}' "$name" "$id"

    if command -v soroban >/dev/null 2>&1 && [[ -n "$id" && "$id" != "$name" ]]; then
      soroban contract invoke --id "$id" --network "$NETWORK" -- get_version_metadata \
        > "$OUT/${name}_version.json" 2>/dev/null || true
    fi
    if [[ -f "$wasm" ]]; then
      cp "$wasm" "$OUT/wasm/"
      if command -v soroban >/dev/null 2>&1; then
        soroban contract inspect --wasm "$wasm" > "$OUT/wasm/${name}.inspect.txt" 2>/dev/null || true
      fi
    fi
  done
  echo
  echo "  ]"
  echo "}"
} > "$OUT/inventory.json"

echo "Backup written to $OUT"
