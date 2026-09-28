#!/usr/bin/env bash
# verify_deploy.sh — post-deployment smoke test (closes #708).
#
# Read-only. Invokes view functions and inspects local WASM. It never writes
# chain state and never needs a signing key, so it is safe to run from CI on a
# schedule.
#
# Usage:
#   ./scripts/deploy/verify_deploy.sh [network] [--config PATH]
#
# Defaults:
#   network = testnet
#   config  = config/${network}_contracts.json
#
# Exit status:
#   0  every check that could run passed
#   1  at least one check failed
#   2  a required input was missing and the run could not start
#
# A contract whose id is blank in the config is reported as SKIPPED, not as a
# pass: an empty id means it was never deployed, and quietly passing it is how
# a broken deploy gets signed off.
#
# Only Soroban CLI subcommands already used elsewhere in this repository are
# invoked here: `network add`, `contract invoke` and `contract inspect`.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

NETWORK="testnet"
CONFIG_OVERRIDE=""
SKIP_COUNT=0

ORIGINAL_ARGS=("$@")
assert_no_secret_args ${ORIGINAL_ARGS[@]+"${ORIGINAL_ARGS[@]}"}

while [ $# -gt 0 ]; do
    case "$1" in
        --config)   CONFIG_OVERRIDE="${2:-}"; shift 2 ;;
        --config=*) CONFIG_OVERRIDE="${1#*=}"; shift ;;
        --help|-h)  sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*)         die "unknown flag '$1'. Try: $0 --help" ;;
        *)          NETWORK="$1"; shift ;;
    esac
done

printf '╔══════════════════════════════════════════════════════════════╗\n'
printf '║  Post-deploy verification — StellarAid (%-8s)         ║\n' "$NETWORK"
printf '╚══════════════════════════════════════════════════════════════╝\n'

# ── Inputs ──────────────────────────────────────────────────────────────────

head1 "Inputs"

require_cmd soroban "see docs/DEPLOYMENT.md"
require_cmd python3 "used to read the config file; no jq dependency is assumed"

if [ "$NETWORK" = "mainnet" ]; then
    CONFIG_FILE="${CONFIG_OVERRIDE:-$REPO_ROOT/config/mainnet_contracts.json}"
else
    CONFIG_FILE="${CONFIG_OVERRIDE:-$REPO_ROOT/config/${NETWORK}_contracts.json}"
fi

if [ ! -f "$CONFIG_FILE" ]; then
    die "config not found: ${CONFIG_FILE#$REPO_ROOT/}. Nothing to verify."
fi
ok "config: ${CONFIG_FILE#$REPO_ROOT/}"

resolve_network "$NETWORK"
soroban_configure_network "$NETWORK"

# ── Read the config ─────────────────────────────────────────────────────────
#
# Emits `name<TAB>id<TAB>wasm` per contract. A malformed config is a hard
# failure rather than a silently empty check list.

CONFIGS="$(python3 - "$CONFIG_FILE" <<'PY'
import json, sys
try:
    with open(sys.argv[1]) as fh:
        cfg = json.load(fh)
except Exception as exc:  # noqa: BLE001 - surfaced verbatim to the operator
    sys.stderr.write("config is not readable JSON: {}\n".format(exc))
    raise SystemExit(1)
contracts = cfg.get("contracts") or {}
if not contracts:
    sys.stderr.write("config lists no contracts\n")
    raise SystemExit(1)
for name, entry in sorted(contracts.items()):
    entry = entry or {}
    print("{}\t{}\t{}".format(name, entry.get("id") or "", entry.get("wasm") or ""))
PY
)" || die "could not read $CONFIG_FILE"

CONFIGURED_COUNT="$(printf '%s\n' "$CONFIGS" | grep -c . || true)"
DEPLOYED_COUNT="$(printf '%s\n' "$CONFIGS" | awk -F'\t' '$2 != ""' | grep -c . || true)"
ok "$DEPLOYED_COUNT of $CONFIGURED_COUNT configured contracts have a contract id"

# ── Per-contract checks ─────────────────────────────────────────────────────
#
# `get_version` is provided by every crate in this workspace through the shared
# `impl_semver_queries!()` macro, so it is the one view call that is safe to
# assume exists everywhere. It also doubles as the network reachability probe:
# an uninitialized contract fails every view call, so a failure here is
# reported as "unreachable OR uninitialized" rather than a false pass.

head1 "Contract checks"

while IFS=$'\t' read -r name cid wasm; do
    [ -n "$name" ] || continue
    info ""
    info "── $name ──"

    if [ -z "$cid" ]; then
        SKIP_COUNT=$((SKIP_COUNT + 1))
        warn "$name: no contract id in the config — SKIPPED (never deployed)"
        continue
    fi

    ver_out="$(soroban contract invoke --id "$cid" --network "$NETWORK" -- get_version 2>&1 || true)"
    if printf '%s' "$ver_out" | grep -qE '"(major|minor|patch)"'; then
        record_pass "$name ($cid) reachable and answers get_version"
    else
        record_fail "$name ($cid) did not answer get_version"
        info "   either the contract is not deployed at that id, the network is"
        info "   wrong, or the contract has not been initialised. Output follows:"
        printf '%s\n' "$ver_out" | head -n 5 | sed 's/^/   /' >&2
    fi

    if [ -n "$wasm" ] && [ -f "$REPO_ROOT/$wasm" ]; then
        if soroban contract inspect --wasm "$REPO_ROOT/$wasm" >/dev/null 2>&1; then
            record_pass "$name local WASM inspected ($(du -h "$REPO_ROOT/$wasm" | cut -f1))"
        else
            record_fail "$name local WASM could not be inspected: $wasm"
        fi
    elif [ -n "$wasm" ]; then
        warn "$name: no local WASM at $wasm — cannot compare the deployed hash"
    fi
done <<< "$CONFIGS"

# ── Contract-specific probes ────────────────────────────────────────────────
#
# Each probe is guarded by a check for the contract being configured, so this
# section degrades cleanly on a partial deployment.

head1 "Contract-specific probes"

while IFS=$'\t' read -r name cid wasm; do
    [ -n "$name" ] && [ -n "$cid" ] || continue

    case "$name" in
        platform_config)
            out="$(soroban contract invoke --id "$cid" --network "$NETWORK" -- get_config 2>&1 || true)"
            if printf '%s' "$out" | grep -q '"admin"'; then
                record_pass "platform_config get_config returns an admin"
            else
                record_fail "platform_config get_config failed"
                printf '%s\n' "$out" | head -n 5 | sed 's/^/   /' >&2
            fi
            ;;
        escrow)
            # Escrow keeps its own pause key, separate from shared::pause, and
            # exposes a native `is_paused`. `false` is the expected steady state
            # and is the cheapest proof that the pause wiring is live.
            out="$(soroban contract invoke --id "$cid" --network "$NETWORK" -- is_paused 2>&1 || true)"
            if printf '%s' "$out" | grep -qx 'false'; then
                record_pass "escrow is_paused == false (contract is running)"
            elif printf '%s' "$out" | grep -qx 'true'; then
                record_fail "escrow is PAUSED. Investigate before routing traffic.
   Resume procedure: docs/EMERGENCY_PROCEDURES.md"
            else
                record_fail "escrow is_paused did not return a boolean"
                printf '%s\n' "$out" | head -n 5 | sed 's/^/   /' >&2
            fi
            ;;
    esac
done <<< "$CONFIGS"

# ── Report ──────────────────────────────────────────────────────────────────

head1 "What this does and does not prove"

cat <<'NOTES'
  PROVES
    * every configured contract id exists on the network and is initialized
      enough to answer a view call;
    * the locally built WASM is a well-formed Soroban contract;
    * escrow is not left paused after a deploy.

  DOES NOT PROVE
    * that a money path works. Donate / escrow / release round trips must be
      driven by the Rust test suite on a local sandbox first, and then by a
      small funded transaction on testnet. A deploy that passes every check
      here can still have a broken escrow release path.

    * that the deployed WASM matches the local WASM. `soroban contract inspect`
      reports the local artefact; comparing it to the on-chain hash is a manual
      step recorded in the change ticket.

    * anything at all about a contract whose id is blank. Those are reported as
      SKIPPED, never as passed.

  Read-only: this script submits no transaction and needs no signing key, so it
  can be run on a schedule from CI.
NOTES

if [ "$SKIP_COUNT" -gt 0 ]; then
    warn "$SKIP_COUNT contract(s) skipped because the config has no id for them"
fi

summarise
