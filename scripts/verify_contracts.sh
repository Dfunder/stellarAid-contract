#!/usr/bin/env bash
# verify_contracts.sh — Post-deployment validation: contract function availability.
#
# For every deployed contract listed in the contracts config, this script invokes
# read-only functions (get_version, get_version_metadata, health_check, etc.) to
# confirm the contract is live and exposing the expected surface.
#
# Usage:
#   ./scripts/verify_contracts.sh [network] [contracts_file]
#
# Defaults:
#   network        = testnet
#   contracts_file = config/${network}_contracts.json
set -euo pipefail

NETWORK="${1:-testnet}"
CONTRACTS_FILE="${2:-config/${NETWORK}_contracts.json}"
EXIT_CODE=0
TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

# ── Helpers ──────────────────────────────────────────────────────────────────

pass() { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail() { FAILED=$((FAILED + 1)); EXIT_CODE=1; echo "  ✗ $1"; }
skip() { SKIPPED=$((SKIPPED + 1)); echo "  – $1 (skipped)"; }

invoke_readonly() {
  local id="$1" fn="$2" label="$3"
  TOTAL=$((TOTAL + 1))
  if soroban contract invoke \
      --id "$id" \
      --network "$NETWORK" \
      --output json \
      -- "$fn" 2>/dev/null; then
    pass "$label"
  else
    fail "$label"
  fi
}

# ── Preflight ────────────────────────────────────────────────────────────────

if ! command -v soroban &>/dev/null; then
  echo "Error: 'soroban' CLI not found."
  exit 1
fi

if [ ! -f "$CONTRACTS_FILE" ]; then
  echo "Error: contracts file not found: $CONTRACTS_FILE"
  exit 1
fi

echo "════════════════════════════════════════════════════════════════"
echo "  Contract Function Availability — $NETWORK"
echo "════════════════════════════════════════════════════════════════"
echo "  Config: $CONTRACTS_FILE"
echo ""

# ── Read contract IDs from JSON config ───────────────────────────────────────
# Supports the flat { "contracts": { "<name>": { "id": "..." } } } format.

CONTRACT_NAMES=$(soroban contract spec --wasm /dev/null 2>/dev/null || true) # noop

# Parse contract names and IDs from the JSON config.
# We use a simple grep+sed approach to stay compatible without jq.
while IFS= read -r line; do
  name=$(echo "$line" | cut -d: -f1 | tr -d ' "')
  id=$(echo "$line" | cut -d: -f2- | tr -d ' ",')
  [ -z "$id" ] && continue

  echo "── $name ($id) ──"

  # Core function checks — every contract exposes these via the semver macro.
  invoke_readonly "$id" "get_version"             "$name.get_version"
  invoke_readonly "$id" "get_version_metadata"    "$name.get_version_metadata"
  invoke_readonly "$id" "health_check"            "$name.health_check"
  invoke_readonly "$id" "get_health_metrics"      "$name.get_health_metrics"
  invoke_readonly "$id" "get_sla_targets"         "$name.get_sla_targets"
  invoke_readonly "$id" "get_alert_config"        "$name.get_alert_config"
  invoke_readonly "$id" "detect_anomaly"          "$name.detect_anomaly"
  invoke_readonly "$id" "is_feature_enabled"      "$name.is_feature_enabled"
  invoke_readonly "$id" "get_rollout_state"       "$name.get_rollout_state"
  invoke_readonly "$id" "should_rollback"         "$name.should_rollback"
  invoke_readonly "$id" "route_to_canary"         "$name.route_to_canary"

  echo ""
done < <(
  grep -oP '"[a-z_]+"\s*:\s*\{\s*"id"\s*:\s*"[^"]*"' "$CONTRACTS_FILE" \
    | sed 's/"id"\s*:\s*"/id:/' \
    | sed 's/^\s*"//' \
    | sed 's/"\s*:\s*{/:/' \
    | grep -v '"id"\s*:\s*""'
)

# ── Summary ──────────────────────────────────────────────────────────────────

TOTAL=$((TOTAL))
echo "════════════════════════════════════════════════════════════════"
echo "  Results:  $PASSED passed, $FAILED failed, $SKIPPED skipped  (total: $TOTAL)"
echo "════════════════════════════════════════════════════════════════"

exit $EXIT_CODE
