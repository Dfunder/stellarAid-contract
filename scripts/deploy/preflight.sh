#!/usr/bin/env bash
# preflight.sh — pre-deployment validation checklist (closes #708).
#
# Run this BEFORE building and before touching a network. It is read-only: it
# never builds, never deploys, and never submits a transaction. It can
# therefore be run in CI, in a container, or on a machine with no credentials
# at all.
#
# Usage:
#   ./scripts/deploy/preflight.sh [network]
#
# Defaults:
#   network = testnet
#
# Exit status:
#   0  every check passed
#   1  at least one check failed (see the tally)
#   2  a required input was missing and the run could not start
#
# Credential handling: this script requires no secret to run. It only checks
# that whatever the deploy step will need is present, and it never prints,
# measures or logs a value.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

NETWORK="${1:-testnet}"
CONFIRM_MAINNET=0

ORIGINAL_ARGS=("$@")
assert_no_secret_args ${ORIGINAL_ARGS[@]+"${ORIGINAL_ARGS[@]}"}

for arg in ${ORIGINAL_ARGS[@]+"${ORIGINAL_ARGS[@]}"}; do
    case "$arg" in
        --confirm-mainnet) CONFIRM_MAINNET=1 ;;
        -*)               die "unknown flag '$arg'. Usage: $0 [network] [--confirm-mainnet]" ;;
    esac
done

printf '╔══════════════════════════════════════════════════════════════╗\n'
printf '║  Pre-deployment checklist — StellarAid (%-8s)          ║\n' "$NETWORK"
printf '╚══════════════════════════════════════════════════════════════╝\n'

# ── 1. Toolchain ────────────────────────────────────────────────────────────
head1 "1. Toolchain"

if command -v cargo &>/dev/null; then
    record_pass "cargo on PATH ($(cargo --version 2>/dev/null || echo 'version unknown'))"
else
    record_fail "cargo not on PATH — https://rustup.rs"
fi

if [ -f "$REPO_ROOT/rust-toolchain.toml" ]; then
    record_pass "rust-toolchain.toml present (channel is authoritative; the Docker image honours it)"
else
    record_fail "rust-toolchain.toml missing at the workspace root"
fi

if rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    record_pass "wasm32-unknown-unknown target installed"
else
    record_fail "wasm32-unknown-unknown target missing — run: rustup target add wasm32-unknown-unknown"
fi

if [ -f "$REPO_ROOT/Cargo.lock" ] && grep -q 'resolver = "2"' "$REPO_ROOT/Cargo.toml"; then
    record_pass "workspace uses resolver = 2 with a committed Cargo.lock"
else
    record_fail "expected resolver = \"2\" and a committed Cargo.lock"
fi

# ── 2. Soroban CLI and network definition ───────────────────────────────────
head1 "2. Network"

if command -v soroban &>/dev/null; then
    record_pass "soroban CLI on PATH ($(soroban --version 2>/dev/null | head -1 || echo 'version unknown'))"
else
    record_fail "soroban CLI not on PATH — cargo install --locked soroban-cli --features opt"
fi

if [ -f "$SOROBAN_CONFIG" ]; then
    record_pass ".soroban/config.toml present"
else
    die "$SOROBAN_CONFIG not found; cannot resolve a network. Aborting before any build."
fi

if [ -n "$(network_rpc_url "$NETWORK")" ] && [ -n "$(network_passphrase "$NETWORK")" ]; then
    record_pass "network '$NETWORK' is defined in .soroban/config.toml (rpc-url and passphrase both present; values not printed)"
else
    record_fail "network '$NETWORK' is not defined in .soroban/config.toml"
fi

# ── 3. Repository hygiene and secrets ───────────────────────────────────────
head1 "3. Secrets and repository hygiene"

if [ -e "$REPO_ROOT/.env" ]; then
    record_fail "a .env file exists in the working tree. It is gitignored, which is correct, but it must not be inside a Docker build context or an artifact. Verify .dockerignore before building."
else
    record_pass "no .env file in the working tree"
fi

if [ -d "$REPO_ROOT/.soroban/accounts" ]; then
    record_fail ".soroban/accounts exists — it holds local key identities. Keep it out of every build context and artifact."
else
    record_pass "no .soroban/accounts directory"
fi

assert_repo_has_no_key_material

# ── 4. Contract inventory ──────────────────────────────────────────────────
head1 "4. Contract inventory"

WORKSPACE_CONTRACTS="$(workspace_contracts)"
ALL_CONTRACTS="$(all_contract_crates)"

info "workspace member contracts (built by 'cargo build --workspace'):"
for c in $WORKSPACE_CONTRACTS; do info "    $c"; done
info ""
info "all contracts with a Soroban entry point, members or not:"
for c in $ALL_CONTRACTS; do info "    $c"; done
info ""

NON_MEMBERS="$(comm -13 <(printf '%s\n' $WORKSPACE_CONTRACTS | sort -u) \
                        <(printf '%s\n' $ALL_CONTRACTS | sort -u) || true)"
if [ -n "$NON_MEMBERS" ]; then
    warn "not registered in the root Cargo.toml [workspace] members list:"
    for c in $NON_MEMBERS; do warn "    $c  — build with: cargo build -p $c --target wasm32-unknown-unknown --release"; done
    info "   (registering them is a separate, deliberate change; this script does not edit Cargo.toml)"
fi

MISSING_WASM=""
for c in $WORKSPACE_CONTRACTS; do
    w="$(wasm_path "$c")"
    if [ -f "$w" ]; then
        record_pass "wasm present: $c ($(du -h "$w" | cut -f1))"
    else
        MISSING_WASM="$MISSING_WASM $c"
    fi
done
if [ -n "$MISSING_WASM" ]; then
    record_fail "wasm missing for:$MISSING_WASM"
    info "   build them with: make build   (cargo build --target wasm32-unknown-unknown --release)"
fi

# ── 5. Deployment configuration file ────────────────────────────────────────
head1 "5. Deployment configuration"

CONFIG_FILE="$REPO_ROOT/config/${NETWORK}_contracts.json"
if [ "$NETWORK" = "mainnet" ]; then
    CONFIG_FILE="$REPO_ROOT/config/mainnet_contracts.json"
fi

if [ -f "$CONFIG_FILE" ]; then
    record_pass "config present: ${CONFIG_FILE#$REPO_ROOT/}"
    if command -v python3 &>/dev/null; then
        if python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$CONFIG_FILE" 2>/dev/null; then
            record_pass "config is valid JSON"
        else
            record_fail "config is not valid JSON"
        fi
    else
        warn "python3 not available — config JSON not validated"
    fi
    # A config with no ids at all is the expected state before a first deploy.
    # A *partially* populated config is the dangerous case: it deploys some
    # contracts and leaves the rest pointing at nothing, so that is the failure.
    TOTAL_IDS="$(grep -c '"id"' "$CONFIG_FILE" || true)"
    FILLED_IDS="$(grep -c '"id"[[:space:]]*:[[:space:]]*"[^"]' "$CONFIG_FILE" || true)"
    if [ "$FILLED_IDS" -eq 0 ]; then
        warn "no contract ids recorded yet — expected before a first deploy"
    elif [ "$FILLED_IDS" -lt "$TOTAL_IDS" ]; then
        record_fail "config is only partially populated ($FILLED_IDS of $TOTAL_IDS ids). Deploy every contract, or remove the blank entries before signing off."
    else
        record_pass "all $TOTAL_IDS contract ids are populated"
    fi
else
    if [ "$NETWORK" = "mainnet" ]; then
        record_fail "config/mainnet_contracts.json does not exist yet. It is intentionally not committed; copy config/testnet_contracts.json, rename it, and fill in the ids by hand."
    else
        record_fail "config/${NETWORK}_contracts.json not found"
    fi
fi

# ── 6. Credentials the deploy step will need ────────────────────────────────
head1 "6. Credentials (presence only — no value is ever printed)"

if [ -n "${STELLAR_DEPLOYER_IDENTITY:-}" ]; then
    record_pass "STELLAR_DEPLOYER_IDENTITY is set (value withheld)"
else
    record_fail "STELLAR_DEPLOYER_IDENTITY is not set. Create a local identity with
       soroban keys generate <name> --network $NETWORK
   and export its name. An identity name is not a secret; the key it points at is."
fi

if [ -n "${STELLAR_DEPLOYER_SECRET:-}" ]; then
    record_pass "STELLAR_DEPLOYER_SECRET is present in this environment (value withheld)"
else
    record_fail "STELLAR_DEPLOYER_SECRET is not set. Inject it from a secret manager into the deploying process only."
fi

if [ "$NETWORK" = "mainnet" ]; then
    head1 "6b. Mainnet approval gate (dry run)"
    if [ "$CONFIRM_MAINNET" -eq 1 ]; then
        record_pass "--confirm-mainnet supplied"
    else
        record_fail "--confirm-mainnet NOT supplied — the deploy script will refuse to run"
    fi
    if [ "${STELLAR_ALLOW_MAINNET:-}" = "1" ]; then
        record_pass "STELLAR_ALLOW_MAINNET=1"
    else
        record_fail "STELLAR_ALLOW_MAINNET is not 1 — the deploy script will refuse to run"
    fi
    if [ "${STELLAR_MAINNET_CONFIRM:-}" = "$(network_passphrase mainnet)" ]; then
        record_pass "STELLAR_MAINNET_CONFIRM matches the mainnet passphrase"
    else
        record_fail "STELLAR_MAINNET_CONFIRM does not match the mainnet passphrase"
    fi
fi

# ── 7. Human sign-off items the script cannot check ─────────────────────────
head1 "7. Human sign-off (cannot be automated)"

cat <<'CHECKLIST'
  [ ] A fresh backup snapshot exists for every in-scope contract
      (scripts/backup_contract_state.sh; see docs/MAINTENANCE_WINDOWS.md §3).
  [ ] The deploy runs inside a maintenance window, or is a declared emergency
      (docs/MAINTENANCE_WINDOWS.md §1).
  [ ] `cargo test --workspace` and `cargo clippy -- -D warnings` are green on the
      exact commit being deployed. This script cannot tell you that.
  [ ] The WASM being deployed was built from that same commit, and its hash has
      been recorded.
  [ ] User-facing status page / Discord notice is drafted
      (docs/COMMUNICATION_TEMPLATES.md).
  [ ] A second engineer has reviewed the plan. Single-operator deploys are not
      permitted for mainnet.
  [ ] The rollback target is known and still live: the previous contract id
      (docs/UPGRADE_AND_ROLLBACK.md).
CHECKLIST

summarise
