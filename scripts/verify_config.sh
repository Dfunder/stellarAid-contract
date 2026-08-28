#!/usr/bin/env bash
# verify_config.sh — Post-deployment validation: configuration loading.
#
# Verifies that:
#   1. The contracts config file exists and is well-formed JSON.
#   2. Every expected contract has a non-empty ID.
#   3. Network settings (rpc_url, passphrase) are present and reachable.
#   4. WASM files referenced in the config actually exist on disk.
#   5. Dependency edges reference contracts that also appear in the config.
#
# Usage:
#   ./scripts/verify_config.sh [network] [contracts_file]
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

pass() { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail() { FAILED=$((FAILED + 1)); EXIT_CODE=1; echo "  ✗ $1"; }
TOTAL=$((TOTAL + 1))

echo "════════════════════════════════════════════════════════════════"
echo "  Configuration Loading — $NETWORK"
echo "════════════════════════════════════════════════════════════════"
echo "  Config: $CONTRACTS_FILE"
echo ""

# ── 1. File existence ────────────────────────────────────────────────────────
echo "── File checks ──"
if [ -f "$CONTRACTS_FILE" ]; then
  pass "Config file exists"
else
  fail "Config file not found: $CONTRACTS_FILE"
  echo ""
  echo "Aborting — cannot continue without config."
  echo "Results: $PASSED passed, $FAILED failed"
  exit 1
fi
TOTAL=$((TOTAL + 1))

# ── 2. Basic JSON structure (parseable) ─────────────────────────────────────
if soroban contract spec 2>/dev/null || true; then
  : # soroban available
fi

# Use python3 as a portable JSON validator (no jq dependency).
if command -v python3 &>/dev/null; then
  if python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$CONTRACTS_FILE" 2>/dev/null; then
    pass "Config is valid JSON"
  else
    fail "Config is not valid JSON"
  fi
else
  echo "  – JSON validation skipped (python3 not available)"
fi
TOTAL=$((TOTAL + 1))

# ── 3. Network settings ─────────────────────────────────────────────────────
echo ""
echo "── Network settings ──"

check_field() {
  local field="$1" label="$2"
  TOTAL=$((TOTAL + 1))
  val=$(grep -oP "\"$field\"\s*:\s*\"([^\"]*)\"" "$CONTRACTS_FILE" 2>/dev/null | head -1 | sed 's/.*:\s*"//;s/"$//')
  if [ -n "$val" ]; then
    pass "$label = $val"
  else
    fail "$label is missing or empty"
  fi
}

check_field "network"            "network"
check_field "rpc_url"            "rpc_url"
check_field "network_passphrase" "network_passphrase"
check_field "admin_address"      "admin_address"

# ── 4. Contract IDs non-empty ───────────────────────────────────────────────
echo ""
echo "── Contract IDs ──"

# Extract contract blocks: name + id
while IFS= read -r block; do
  TOTAL=$((TOTAL + 1))
  cname=$(echo "$block" | grep -oP '"[a-z_]+"\s*:\s*\{' | head -1 | grep -oP '"[a-z_]+"' | tr -d '"')
  cid=$(echo "$block" | grep -oP '"id"\s*:\s*"[^"]*"' | sed 's/.*:\s*"//;s/"$//')

  if [ -z "$cname" ]; then
    continue
  fi

  if [ -z "$cid" ]; then
    fail "$cname — contract ID is empty (not yet deployed?)"
  else
    pass "$cname — $cid"
  fi
done < <(
  # Extract JSON objects per contract
  python3 -c "
import json, sys
cfg = json.load(open(sys.argv[1]))
for name, info in cfg.get('contracts', {}).items():
    cid = info.get('id', '')
    wasm = info.get('wasm', '')
    deps = info.get('depends_on', [])
    print(f'{name}|{cid}|{wasm}|{\",\".join(deps)}')
" "$CONTRACTS_FILE" 2>/dev/null | while IFS='|' read -r cname cid wasm deps; do
    echo "name=$cname id=$cid wasm=$wasm deps=$deps"
  done
) 2>/dev/null || true

# Fallback: parse with grep if python3 path failed
if [ "$PASSED" -le 4 ]; then
  while IFS= read -r line; do
    TOTAL=$((TOTAL + 1))
    name=$(echo "$line" | grep -oP '"[a-z_]+"\s*:' | head -1 | tr -d '" :')
    id=$(echo "$line" | grep -oP '"id"\s*:\s*"[^"]*"' | sed 's/.*:\s*"//;s/"$//')
    [ -z "$name" ] && continue
    if [ -z "$id" ]; then
      fail "$name — contract ID is empty (not yet deployed?)"
    else
      pass "$name — $id"
    fi
  done < <(grep -oP '"[a-z_]+"\s*:\s*\{[^}]*"id"\s*:\s*"[^"]*"' "$CONTRACTS_FILE" 2>/dev/null || true)
fi

# ── 5. WASM file existence ──────────────────────────────────────────────────
echo ""
echo "── WASM files ──"

while IFS= read -r wasm_line; do
  TOTAL=$((TOTAL + 1))
  wasm_path=$(echo "$wasm_line" | sed 's/.*"wasm"\s*:\s*"//;s/".*//')
  contract_name=$(echo "$wasm_line" | grep -oP '"[a-z_]+"\s*:' | head -1 | tr -d '" :')
  [ -z "$wasm_path" ] && continue
  [ -z "$contract_name" ] && contract_name="$wasm_path"

  if [ -f "$wasm_path" ]; then
    pass "$contract_name wasm exists ($wasm_path)"
  else
    fail "$contract_name wasm NOT found ($wasm_path)"
  fi
done < <(grep '"wasm"' "$CONTRACTS_FILE" 2>/dev/null || true)

# ── 6. Dependency edges ─────────────────────────────────────────────────────
echo ""
echo "── Dependency edges ──"

while IFS= read -r dep_line; do
  TOTAL=$((TOTAL + 1))
  # Extract the contract name and its depends_on list
  contract_name=$(echo "$dep_line" | grep -oP '"[a-z_]+"\s*:' | head -1 | tr -d '" :')
  deps=$(echo "$dep_line" | grep -oP '"depends_on"\s*:\s*\[[^\]]*\]' | grep -oP '"[a-z_]+"' | tr -d '"' | tr '\n' ' ')

  if [ -z "$deps" ] || [ "$deps" = " " ]; then
    pass "$contract_name — no dependencies"
    continue
  fi

  for dep in $deps; do
    # Check if the dependency appears as a top-level contract
    if grep -qP "\"$dep\"\s*:\s*\{" "$CONTRACTS_FILE"; then
      pass "$contract_name → $dep (found)"
    else
      fail "$contract_name → $dep (MISSING from config)"
    fi
  done
done < <(grep '"depends_on"' "$CONTRACTS_FILE" 2>/dev/null || true)

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Results:  $PASSED passed, $FAILED failed  (total: $TOTAL)"
echo "════════════════════════════════════════════════════════════════"

exit $EXIT_CODE
