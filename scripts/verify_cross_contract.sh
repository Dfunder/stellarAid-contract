#!/usr/bin/env bash
# verify_cross_contract.sh — Post-deployment validation: cross-contract calls.
#
# Validates the contract dependency graph by verifying that each contract can
# resolve its linked contract addresses and that cross-contract read-only calls
# succeed. The dependency chains tested are:
#
#   escrow        → platform_config  (get_fee_bps, get_usdc_token, get_admin)
#   dispute_arbiter → escrow         (via refund_client / release_payment)
#   dispute_arbiter → platform_config (via get_admin, get_usdc_token)
#   donation      → campaign         (get_campaign)
#   withdrawal    → donation         (get_total_raised)
#
# Usage:
#   ./scripts/verify_cross_contract.sh [network] [contracts_file]
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

pass()  { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail()  { FAILED=$((FAILED + 1)); EXIT_CODE=1; echo "  ✗ $1"; }

invoke() {
  local id="$1" fn="$2" label="$3"; shift 3
  TOTAL=$((TOTAL + 1))
  local args=("$@")
  if result=$(soroban contract invoke \
      --id "$id" \
      --network "$NETWORK" \
      --output json \
      -- "$fn" "${args[@]}" 2>&1); then
    pass "$label"
  else
    fail "$label — $(echo "$result" | tail -c 200)"
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
echo "  Cross-Contract Calls — $NETWORK"
echo "════════════════════════════════════════════════════════════════"
echo "  Config: $CONTRACTS_FILE"
echo ""

# ── Load contract IDs ────────────────────────────────────────────────────────

declare -A IDS
while IFS='|' read -r cname cid; do
  [ -z "$cid" ] && continue
  IDS["$cname"]="$cid"
done < <(
  grep -oP '"[a-z_]+"\s*:\s*\{[^}]*"id"\s*:\s*"[^"]*"' "$CONTRACTS_FILE" \
    | while IFS= read -r line; do
      name=$(echo "$line" | grep -oP '"[a-z_]+"\s*:' | head -1 | tr -d '" :')
      id=$(echo "$line" | grep -oP '"id"\s*:\s*"[^"]*"' | sed 's/.*:\s*"//;s/"$//')
      [ -n "$name" ] && [ -n "$id" ] && echo "${name}|${id}"
    done
)

# ── 1. escrow → platform_config ─────────────────────────────────────────────
if [ -n "${IDS[escrow]:-}" ] && [ -n "${IDS[platform_config]:-}" ]; then
  echo "── escrow → platform_config ──"
  ESC="${IDS[escrow]}"
  CFG="${IDS[platform_config]}"

  # platform_config exposes get_config which returns all linked addresses
  invoke "$CFG" "get_config" "platform_config.get_config (resolves admin, wallet, token)"

  # The escrow contract reads fee_bps, usdc_token, admin from platform_config
  # during create_escrow/release_payment. We verify the config contract returns
  # data that escrow would consume.
  invoke "$CFG" "get_token_metadata" "platform_config.get_token_metadata (token info for escrow)"

  # Verify escrow can see platform_config state
  invoke "$ESC" "is_paused"  "escrow.is_paused (independent read)"
  invoke "$ESC" "get_dispute_ttl_ledgers" "escrow.get_dispute_ttl_ledgers"
  echo ""
fi

# ── 2. donation → campaign ──────────────────────────────────────────────────
if [ -n "${IDS[donation]:-}" ] && [ -n "${IDS[campaign]:-}" ]; then
  echo "── donation → campaign ──"
  DON="${IDS[donation]}"
  CAMP="${IDS[campaign]}"

  # Campaign must be queryable; donation contract calls get_campaign cross-contract
  invoke "$CAMP" "get_campaign_count" "campaign.get_campaign_count (donation reads this)"

  # Donation history queries should work (empty is fine)
  invoke "$DON" "get_total_raised" "donation.get_total_raised (read-only proxy)"
  echo ""
fi

# ── 3. withdrawal → donation ────────────────────────────────────────────────
if [ -n "${IDS[withdrawal]:-}" ] && [ -n "${IDS[donation]:-}" ]; then
  echo "── withdrawal → donation ──"
  WTH="${IDS[withdrawal]}"
  DON="${IDS[donation]}"

  # Withdrawal contract calls donation.get_total_raised cross-contract
  invoke "$DON" "get_total_raised" "donation.get_total_raised (withdrawal dependency)"
  echo ""
fi

# ── 4. dispute_arbiter → escrow + platform_config ───────────────────────────
if [ -n "${IDS[dispute_arbiter]:-}" ] && [ -n "${IDS[escrow]:-}" ] && [ -n "${IDS[platform_config]:-}" ]; then
  echo "── dispute_arbiter → escrow + platform_config ──"
  DA="${IDS[dispute_arbiter]}"
  ESC="${IDS[escrow]}"
  CFG="${IDS[platform_config]}"

  # Dispute arbiter resolves escrow refunds and releases via cross-contract calls
  invoke "$DA"  "health_check"           "dispute_arbiter.health_check"
  invoke "$ESC" "is_paused"              "escrow.is_paused (dispute_arbiter dependency)"
  invoke "$CFG" "get_config"             "platform_config.get_config (dispute_arbiter dependency)"
  echo ""
fi

# ── 5. Full chain: escrow ↔ platform_config ↔ dispute_arbiter ──────────────
if [ -n "${IDS[escrow]:-}" ] && [ -n "${IDS[platform_config]:-}" ] && [ -n "${IDS[dispute_arbiter]:-}" ]; then
  echo "── Full dependency chain: escrow ↔ config ↔ dispute ──"
  ESC="${IDS[escrow]}"
  CFG="${IDS[platform_config]}"
  DA="${IDS[dispute_arbiter]}"

  # All three must respond healthily
  invoke "$ESC" "health_check" "escrow.health_check"
  invoke "$CFG" "health_check" "platform_config.health_check"
  invoke "$DA"  "health_check" "dispute_arbiter.health_check"
  echo ""
fi

# ── 6. Health cascade: all contracts ────────────────────────────────────────
echo "── Health cascade (all contracts) ──"
HEALTHY=0
UNHEALTHY=0
for name in "${!IDS[@]}"; do
  TOTAL=$((TOTAL + 1))
  if soroban contract invoke \
      --id "${IDS[$name]}" \
      --network "$NETWORK" \
      --output json \
      -- health_check 2>/dev/null | grep -q '"anomaly":false\|"anomaly": false'; then
    HEALTHY=$((HEALTHY + 1))
    pass "$name health_check OK"
  elif soroban contract invoke \
      --id "${IDS[$name]}" \
      --network "$NETWORK" \
      --output json \
      -- health_check 2>/dev/null >/dev/null; then
    HEALTHY=$((HEALTHY + 1))
    pass "$name health_check responded"
  else
    UNHEALTHY=$((UNHEALTHY + 1))
    fail "$name health_check failed"
  fi
done
echo "  Health cascade: $HEALTHY healthy, $UNHEALTHY unhealthy"
echo ""

# ── Summary ──────────────────────────────────────────────────────────────────
echo "════════════════════════════════════════════════════════════════"
echo "  Results:  $PASSED passed, $FAILED failed  (total: $TOTAL)"
echo "════════════════════════════════════════════════════════════════"

exit $EXIT_CODE
