#!/usr/bin/env bash
# preflight.sh — the gate every deploy has to pass.
#
# Closes #888.
#
# WHAT THIS IS
#   scripts/deploy/preflight.sh (closes #708) checks the *deploy environment*:
#   toolchain, .soroban/config.toml, credentials, contract inventory, the
#   deployment config file. It is read-only and fast, and it deliberately does
#   not build or test, because it has to be runnable on a machine with no
#   network and no credentials.
#
#   That leaves a gap: nothing verified that the commit about to be deployed
#   actually compiles, passes its tests, is formatted, and is free of clippy
#   warnings — and nothing verified the target network is reachable. Both are
#   cheap to check and expensive to discover mid-deploy.
#
#   This script closes that gap. It runs the repository's own gates (fmt,
#   clippy, test, WASM build), checks the environment variables the deploy will
#   need, probes the target network, and then hands off to the deploy
#   preflight so the two compose rather than duplicate.
#
#   It is a superset: running only this script is sufficient, and it is what
#   `make preflight` and the deploy scripts call.
#
# USAGE
#   scripts/preflight.sh [options] [network]
#
# Options
#   --fast              skip the slow gates (clippy, test, wasm build) and run
#                       only the environment, credential and network checks.
#                       Useful as a quick "is my machine set up" probe; NOT a
#                       substitute for a full run before deploying.
#   --no-network-check  skip the RPC probe. Required in an air-gapped or
#                       firewalled environment where the probe would hang.
#   --skip-deploy-preflight
#                       run only this script's own checks, not the
#                       scripts/deploy/preflight.sh handoff.
#   --timeout N         seconds to allow for the cargo gates (default 3600).
#
# Exit status
#   0  every check passed
#   1  at least one check failed (see the tally)
#   2  a required input was missing and the run could not start
#
# CREDENTIAL HANDLING
#   Same policy as scripts/deploy/lib.sh, which this script sources: no
#   secret is ever accepted as a command argument, printed, measured or
#   logged. Diagnostics name the variable and nothing else.
#
# WHY NOT JUST WIRE `make preflight` TO THE EXISTING SCRIPT
#   Because the existing script cannot answer the questions this one asks, and
#   quietly widening it would have made the fast, offline, credential-free
#   path into something that needs a full toolchain and a live network.
#   Composition keeps both properties: `scripts/deploy/preflight.sh` stays
#   runnable anywhere, this one is the full gate.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/deploy/lib.sh"

FAST=0
NETWORK_CHECK=1
HANDOFF=1
TIMEOUT=3600
NETWORK=""

ORIGINAL_ARGS=("$@")
assert_no_secret_args ${ORIGINAL_ARGS[@]+"${ORIGINAL_ARGS[@]}"}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --fast)                     FAST=1 ;;
        --no-network-check)         NETWORK_CHECK=0 ;;
        --skip-deploy-preflight)    HANDOFF=0 ;;
        --timeout)                  shift; TIMEOUT="${1:-}" ;;
        --timeout=*)                TIMEOUT="${1#--timeout=}" ;;
        -h|--help)                  sed -n '2,/^set -uo/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*)                         die "unknown flag '$1'. See: $0 --help" ;;
        *)                          NETWORK="$1" ;;
    esac
    shift
done

if ! [[ "$TIMEOUT" =~ ^[0-9]+$ ]] || [ "$TIMEOUT" -le 0 ]; then
    die "--timeout expects a positive number of seconds, got '$TIMEOUT'"
fi

# Same default as the deploy preflight, so the two agree when no network is
# named. Never a fallback for a network that was named but is not configured:
# that case is an error in section 2 below, not a silent substitution.
NETWORK="${NETWORK:-testnet}"

printf '╔══════════════════════════════════════════════════════════════╗\n'
printf '║  Preflight — build, test, environment, network   (%-8s)  ║\n' "$NETWORK"
printf '╚══════════════════════════════════════════════════════════════╝\n'
if [ "$FAST" -eq 1 ]; then
    warn "--fast: skipping clippy, test and WASM build. This is NOT a pre-deploy gate."
fi

# Run a command with a timeout, reporting pass/fail rather than aborting.
run_gate() {
    # run_gate <label> <command...>
    local label="$1"; shift
    local start elapsed rc
    start="$(date +%s)"

    head1 "$label"
    info "running: $*"
    if [ "$FAST" -eq 1 ]; then
        warn "skipped (--fast)"
        return 0
    fi

    if command -v gtimeout >/dev/null 2>&1; then
        gtimeout "$TIMEOUT" "$@"
        rc=$?
    elif command -v timeout >/dev/null 2>&1; then
        timeout "$TIMEOUT" "$@"
        rc=$?
    else
        warn "no timeout(1) available (macOS without coreutils); running unbounded"
        "$@"
        rc=$?
    fi
    elapsed=$(( $(date +%s) - start ))

    if [ "$rc" -eq 0 ]; then
        record_pass "$label ($((elapsed / 60))m$((elapsed % 60))s)"
    elif [ "$rc" -eq 124 ]; then
        record_fail "$label timed out after ${TIMEOUT}s"
    else
        record_fail "$label failed (exit $rc, $((elapsed / 60))m$((elapsed % 60))s)"
    fi
    return 0
}

# ── 1. Environment variables ────────────────────────────────────────────────
head1 "1. Environment variables (presence only — no value is ever printed)"

# Every secret the deploy pipeline reads. Checked for presence here so the
# failure is a preflight line rather than a confusing error from halfway
# through a soroban invocation.
#
# SOROBAN_CONFIG is deliberately NOT in this list: it is a path, not a secret,
# and reporting it as "value withheld" is misleading. The deploy preflight
# already checks it, in section 2, where a missing config belongs.
REQUIRED_VARS="STELLAR_DEPLOYER_IDENTITY STELLAR_DEPLOYER_SECRET"
OPTIONAL_VARS="STELLAR_ALLOW_MAINNET STELLAR_MAINNET_CONFIRM"

for var in $REQUIRED_VARS; do
    if [ -n "${!var:-}" ]; then
        record_pass "$var is set (value withheld)"
    else
        record_fail "$var is not set. Export it from a secret manager into this
       process only — never as a command argument, never in a file."
    fi
done

for var in $OPTIONAL_VARS; do
    if [ -n "${!var:-}" ]; then
        record_pass "$var is set (value withheld)"
    else
        info "$var is unset (optional for a $NETWORK deploy)"
    fi
done

if [ "$NETWORK" = "mainnet" ]; then
    if [ "${STELLAR_ALLOW_MAINNET:-}" = "1" ]; then
        record_pass "STELLAR_ALLOW_MAINNET=1"
    else
        record_fail "STELLAR_ALLOW_MAINNET is not 1 — the deploy script will refuse to run"
    fi
    if [ -n "${STELLAR_MAINNET_CONFIRM:-}" ] \
       && [ "${STELLAR_MAINNET_CONFIRM}" = "$(network_passphrase mainnet)" ]; then
        record_pass "STELLAR_MAINNET_CONFIRM matches the mainnet passphrase"
    else
        record_fail "STELLAR_MAINNET_CONFIRM does not match the mainnet passphrase in $SOROBAN_CONFIG"
    fi
fi

# ── 2. Network reachability ─────────────────────────────────────────────────
head1 "2. Network reachability ($NETWORK)"

if [ "$NETWORK_CHECK" -eq 0 ]; then
    warn "network probe skipped (--no-network-check)"
else
    RPC_URL="$(network_rpc_url "$NETWORK")"
    if [ -z "$RPC_URL" ]; then
        record_fail "network '$NETWORK' is not defined in $SOROBAN_CONFIG"
    else
        info "rpc-url: $RPC_URL"
        # A Soroban RPC answers a POST to / with getHealth. Any HTTP answer
        # at all proves reachability; this is deliberately not a full RPC
        # conformance check, because the preflight must not depend on the
        # endpoint agreeing with a particular SDK version.
        PROBE=5
        if command -v curl >/dev/null 2>&1; then
            HEALTH="$(curl -sS --max-time "$PROBE" -o /dev/null -w '%{http_code}' \
                -X POST -H 'Content-Type: application/json' \
                --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
                "$RPC_URL" 2>/dev/null || true)"
            if [ -n "$HEALTH" ] && [ "$HEALTH" != "000" ]; then
                record_pass "network reachable (HTTP $HEALTH from ${PROBE}s probe)"
            else
                record_fail "network NOT reachable within ${PROBE}s. A deploy against an
       unreachable endpoint will fail after the transaction is already signed."
            fi
        else
            warn "curl not available — cannot probe the network (CI runners have it)"
        fi
    fi
fi

# ── 3. Repository gates ─────────────────────────────────────────────────────
# The same commands CI runs. `cargo fmt --all -- --check` and clippy are
# deliberately run from $REPO_ROOT rather than through the scripts/ helpers,
# because those helpers are separately tracked and may be mid-fix.
run_gate "3a. cargo fmt --all -- --check" \
    cargo fmt --all -- --check

run_gate "3b. cargo clippy --workspace --all-targets -- -D warnings" \
    cargo clippy --workspace --all-targets -- -D warnings

run_gate "3c. cargo test --workspace --no-fail-fast" \
    cargo test --workspace --no-fail-fast

# ── 4. WASM artifacts ───────────────────────────────────────────────────────
head1 "4. WASM artifacts"

run_gate "4a. cargo build --workspace --target wasm32-unknown-unknown --release" \
    cargo build --locked --workspace --target wasm32-unknown-unknown --release

WORKSPACE_CONTRACTS="$(workspace_contracts)"
if [ -z "$WORKSPACE_CONTRACTS" ]; then
    record_fail "could not read the [workspace] members list from Cargo.toml"
else
    missing=0
    for c in $WORKSPACE_CONTRACTS; do
        w="$(wasm_path "$c")"
        if [ -f "$w" ]; then
            record_pass "wasm present: $c ($(du -h "$w" | cut -f1))"
        else
            missing=$((missing + 1))
            info "missing: $c  ->  $w"
        fi
    done
    if [ "$missing" -ne 0 ]; then
        record_fail "$missing of $(printf '%s\n' $WORKSPACE_CONTRACTS | wc -l | tr -d ' ') workspace contracts have no WASM artifact"
    fi
fi

# ── 5. Handoff to the deploy-environment preflight ──────────────────────────
if [ "$HANDOFF" -eq 1 ]; then
    head1 "5. Deploy-environment preflight (scripts/deploy/preflight.sh)"
    DEPLOY_RC=0
    bash "$REPO_ROOT/scripts/deploy/preflight.sh" "$NETWORK" || DEPLOY_RC=$?
    if [ "$DEPLOY_RC" -eq 0 ]; then
        record_pass "deploy-environment preflight passed"
    else
        record_fail "deploy-environment preflight failed (exit $DEPLOY_RC)"
    fi
else
    head1 "5. Deploy-environment preflight"
    info "skipped (--skip-deploy-preflight)"
fi

# ── Verdict ─────────────────────────────────────────────────────────────────
summarise
