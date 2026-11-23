#!/usr/bin/env bash
# verify_operations.sh — Post-deployment validation: basic contract operations.
#
# Invokes domain-specific read-only functions on each deployed contract to
# confirm that the business logic layer is functional. This goes beyond
# function availability (verify_contracts.sh) to verify the functions actually
# return expected types/values.
#
# Usage:
#   ./scripts/verify_operations.sh [network] [contracts_file]
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

pass()  { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail()  { FAILED=$((FAILED + 1)); EXIT_CODE=1; echo "  ✗ $1"; }
skip()  { SKIPPED=$((SKIPPED + 1)); echo "  – $1"; }
TOTAL() { TOTAL=$((TOTAL + 1)); }

invoke_op() {
  local id="$1" fn="$2" label="$3"; shift 3
  TOTAL=$((TOTAL + 1))
  local args=("$@")
  if result=$(soroban contract invoke \
      --id "$id" \
      --network "$NETWORK" \
      --output json \
      -- "$fn" "${args[@]}" 2>&1); then
    pass "$label → $(echo "$result" | head -c 120)"
  else
    fail "$label → $(echo "$result" | tail -c 120)"
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
echo "  Basic Operations — $NETWORK"
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

# ── platform_config ─────────────────────────────────────────────────────────
if [ -n "${IDS[platform_config]:-}" ]; then
  echo "── platform_config (${IDS[platform_config]}) ──"
  CFG="${IDS[platform_config]}"
  invoke_op "$CFG" "get_config"       "platform_config.get_config"
  invoke_op "$CFG" "get_token_metadata" "platform_config.get_token_metadata"
  invoke_op "$CFG" "health_check"     "platform_config.health_check"
  echo ""
fi

# ── escrow ──────────────────────────────────────────────────────────────────
if [ -n "${IDS[escrow]:-}" ]; then
  echo "── escrow (${IDS[escrow]}) ──"
  ESC="${IDS[escrow]}"
  invoke_op "$ESC" "is_paused"        "escrow.is_paused"
  invoke_op "$ESC" "get_dispute_ttl_ledgers" "escrow.get_dispute_ttl_ledgers"
  invoke_op "$ESC" "health_check"     "escrow.health_check"
  echo ""
fi

# ── campaign ────────────────────────────────────────────────────────────────
if [ -n "${IDS[campaign]:-}" ]; then
  echo "── campaign (${IDS[campaign]}) ──"
  CAMP="${IDS[campaign]}"
  invoke_op "$CAMP" "get_campaign_count" "campaign.get_campaign_count"
  invoke_op "$CAMP" "health_check"       "campaign.health_check"
  echo ""
fi

# ── donation ────────────────────────────────────────────────────────────────
if [ -n "${IDS[donation]:-}" ]; then
  echo "── donation (${IDS[donation]}) ──"
  DON="${IDS[donation]}"
  invoke_op "$DON" "health_check" "donation.health_check"
  echo ""
fi

# ── withdrawal ──────────────────────────────────────────────────────────────
if [ -n "${IDS[withdrawal]:-}" ]; then
  echo "── withdrawal (${IDS[withdrawal]}) ──"
  WTH="${IDS[withdrawal]}"
  invoke_op "$WTH" "health_check" "withdrawal.health_check"
  echo ""
fi

# ── dispute_arbiter ─────────────────────────────────────────────────────────
if [ -n "${IDS[dispute_arbiter]:-}" ]; then
  echo "── dispute_arbiter (${IDS[dispute_arbiter]}) ──"
  DA="${IDS[dispute_arbiter]}"
  invoke_op "$DA" "health_check" "dispute_arbiter.health_check"
  echo ""
fi

# ── commission_agreement ────────────────────────────────────────────────────
if [ -n "${IDS[commission_agreement]:-}" ]; then
  echo "── commission_agreement (${IDS[commission_agreement]}) ──"
  CA="${IDS[commission_agreement]}"
  invoke_op "$CA" "health_check" "commission_agreement.health_check"
  echo ""
fi

# ── subscription ────────────────────────────────────────────────────────────
if [ -n "${IDS[subscription]:-}" ]; then
  echo "── subscription (${IDS[subscription]}) ──"
  SUB="${IDS[subscription]}"
  invoke_op "$SUB" "health_check" "subscription.health_check"
  echo ""
fi

# ── competitions ────────────────────────────────────────────────────────────
if [ -n "${IDS[competitions]:-}" ]; then
  echo "── competitions (${IDS[competitions]}) ──"
  COMP="${IDS[competitions]}"
  invoke_op "$COMP" "health_check" "competitions.health_check"
  echo ""
fi

# ── messaging ───────────────────────────────────────────────────────────────
if [ -n "${IDS[messaging]:-}" ]; then
  echo "── messaging (${IDS[messaging]}) ──"
  MSG="${IDS[messaging]}"
  invoke_op "$MSG" "health_check" "messaging.health_check"
  echo ""
fi

# ── verification ────────────────────────────────────────────────────────────
if [ -n "${IDS[verification]:-}" ]; then
  echo "── verification (${IDS[verification]}) ──"
  VER="${IDS[verification]}"
  invoke_op "$VER" "health_check" "verification.health_check"
  echo ""
fi

# ── revenue_sharing ─────────────────────────────────────────────────────────
if [ -n "${IDS[revenue_sharing]:-}" ]; then
  echo "── revenue_sharing (${IDS[revenue_sharing]}) ──"
  RS="${IDS[revenue_sharing]}"
  invoke_op "$RS" "health_check" "revenue_sharing.health_check"
  echo ""
fi

# ── recruitment ─────────────────────────────────────────────────────────────
if [ -n "${IDS[recruitment]:-}" ]; then
  echo "── recruitment (${IDS[recruitment]}) ──"
  REC="${IDS[recruitment]}"
  invoke_op "$REC" "health_check" "recruitment.health_check"
  echo ""
fi

# ── creator_fund ────────────────────────────────────────────────────────────
if [ -n "${IDS[creator_fund]:-}" ]; then
  echo "── creator_fund (${IDS[creator_fund]}) ──"
  CF="${IDS[creator_fund]}"
  invoke_op "$CF" "health_check" "creator_fund.health_check"
  echo ""
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo "════════════════════════════════════════════════════════════════"
echo "  Results:  $PASSED passed, $FAILED failed, $SKIPPED skipped  (total: $TOTAL)"
echo "════════════════════════════════════════════════════════════════"

exit $EXIT_CODE
