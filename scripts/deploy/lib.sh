#!/usr/bin/env bash
# lib.sh — shared helpers for the scripts/deploy/ pipeline (closes #708).
#
# Sourced by preflight.sh, deploy_testnet.sh, verify_deploy.sh and
# deploy_mainnet.sh. Not executable on its own.
#
# Credential policy (non-negotiable, enforced here rather than per script):
#   * No secret, key, mnemonic or seed phrase is ever stored in this repository.
#   * Secrets are read from the environment, or from an external secret source
#     that populates the environment, at RUN time.
#   * Secrets are never accepted as command-line arguments, so they cannot leak
#     into `ps`, shell history, CI logs or a process listing.
#   * Secret *values* are never printed. Not in full, not masked, not
#     length-prefixed. Diagnostics name the variable and nothing else.
#   * A missing credential is a hard failure. Nothing here falls back to a
#     default identity, a generated key, or an empty value.
set -euo pipefail

# ── Output ──────────────────────────────────────────────────────────────────

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_RED=$'\033[31m'; C_GRN=$'\033[32m'; C_YEL=$'\033[33m'
    C_BLD=$'\033[1m'; C_OFF=$'\033[0m'
else
    C_RED=''; C_GRN=''; C_YEL=''; C_BLD=''; C_OFF=''
fi

log()   { printf '%s\n' "$*"; }
info()  { printf '  %s\n' "$*"; }
ok()    { printf '  %s✓%s %s\n' "$C_GRN" "$C_OFF" "$*"; }
warn()  { printf '  %s! %s%s\n' "$C_YEL" "$*" "$C_OFF" >&2; }
err()   { printf '  %s✗ %s%s\n' "$C_RED" "$*" "$C_OFF" >&2; }
head1() { printf '\n%s%s%s\n' "$C_BLD" "$*" "$C_OFF"; }

# True when the shell is running with `set -x`.
tracing_enabled() {
    case "$-" in *x*) return 0 ;; *) return 1 ;; esac
}

# Fail closed. Every error path in the pipeline ends here.
die() {
    err "$*"
    exit 1
}

PASS_COUNT=0
FAIL_COUNT=0

record_pass() { PASS_COUNT=$((PASS_COUNT + 1)); ok "$1"; }
record_fail() { FAIL_COUNT=$((FAIL_COUNT + 1)); err "$1"; }

# Print the tally and exit non-zero if anything failed. Callers must end with
# this so CI cannot mistake a partial run for a clean one.
summarise() {
    head1 "Result: $PASS_COUNT passed, $FAIL_COUNT failed"
    if [ "$FAIL_COUNT" -ne 0 ]; then
        exit 1
    fi
}

# ── Prerequisite checks ─────────────────────────────────────────────────────

require_cmd() {
    command -v "$1" &>/dev/null || die "required command '$1' is not on PATH. ${2:-}"
}

# ── Credential handling ─────────────────────────────────────────────────────

# Refuse a secret that was passed as a positional argument. This is the guard
# that keeps secrets out of `ps` output and out of shell history.
assert_no_secret_args() {
    for arg in "$@"; do
        case "$arg" in
            S=*|--secret=*|--secret-key=*|--mnemonic=*|--seed=*)
                die "refusing a credential passed on the command line ('${arg%%=*}'). \
Export it as an environment variable instead — see docs/DEPLOYMENT.md."
                ;;
        esac
    done
}

# Require a credential to be present in the environment.
#
# Prints only the variable NAME. Never prints, logs, exports into a subshell
# trace, or measures the value. `set -x` is disabled around the test so a
# traced shell cannot leak it either.
require_secret_env() {
    local var_name="$1"
    local purpose="${2:-this operation}"
    local was_tracing=0

    if tracing_enabled; then
        was_tracing=1
    fi
    # Evaluate the presence test with tracing suppressed, so a shell started
    # with `set -x` cannot echo the value into the log.
    set +x
    if [ -z "${!var_name:-}" ]; then
        if [ "$was_tracing" -eq 1 ]; then set -x; fi
        die "environment variable $var_name is not set; ${purpose} needs it.
  Provide it from a secret manager or your shell, e.g.
    export ${var_name}=\"...\"
Do not commit it, do not pass it as an argument, and do not paste it into a file."
    fi
    if [ "$was_tracing" -eq 1 ]; then set -x; fi
    # Confirm presence only. The value is never printed, measured or logged.
    ok "$var_name is set (value withheld)"
}

# Refuse to run if the repository contains anything that looks like key
# material. Cheap, and it catches the common mistake of committing a key before
# deploying with it.
assert_repo_has_no_key_material() {
    local found

    if ! command -v git &>/dev/null || ! git -C "$REPO_ROOT" rev-parse --git-dir &>/dev/null; then
        warn "not a git checkout (or git is absent) — cannot scan tracked files for key material"
        return 0
    fi

    found="$(git -C "$REPO_ROOT" ls-files 2>/dev/null \
        | grep -Ei '(^|/)(\.env(\..*)?|mnemonic\.txt|[^/]*\.(pem|key|keystore|seed))$' \
        | head -n 5 || true)"
    if [ -n "$found" ]; then
        err "key material appears to be tracked in git:"
        printf '    %s\n' $found >&2
        die "remove these from version control before deploying."
    fi
    ok "no key material tracked in git"
}

# ── Network configuration ───────────────────────────────────────────────────
#
# Values are READ from the committed .soroban/config.toml rather than duplicated
# here, so there is exactly one source of truth and no invented passphrase.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SOROBAN_CONFIG="${SOROBAN_CONFIG:-$REPO_ROOT/.soroban/config.toml}"

network_rpc_url() {
    local network="$1"
    awk -v want="$network" '
        /^[[:space:]]*name[[:space:]]*=/ {
            sub(/^[^=]*=/, ""); name = $0
            gsub(/[[:space:]"]/, "", name); next
        }
        /^[[:space:]]*rpc_url[[:space:]]*=/ {
            sub(/^[^=]*=/, ""); url = $0
            gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", url)
            if (name == want) { print url; exit }
        }
    ' "$SOROBAN_CONFIG"
}

network_passphrase() {
    local network="$1"
    awk -v want="$network" '
        /^[[:space:]]*name[[:space:]]*=/ {
            sub(/^[^=]*=/, ""); name = $0
            gsub(/[[:space:]"]/, "", name); next
        }
        /^[[:space:]]*network_passphrase[[:space:]]*=/ {
            sub(/^[^=]*=/, ""); pp = $0
            gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", pp)
            if (name == want) { print pp; exit }
        }
    ' "$SOROBAN_CONFIG"
}

# Names of every network declared in .soroban/config.toml, space separated.
configured_networks() {
    awk -F'=' '/^[[:space:]]*name[[:space:]]*=/ { print $2 }' "$SOROBAN_CONFIG" \
        | tr -d '"' \
        | tr '\n' ' '
}

# Fail closed when the network is unknown. Never fall back to a default RPC URL
# or passphrase.
resolve_network() {
    local network="$1"
    local known
    RPC_URL="$(network_rpc_url "$network")"
    PASSPHRASE="$(network_passphrase "$network")"
    if [ -z "$RPC_URL" ] || [ -z "$PASSPHRASE" ]; then
        known="$(configured_networks)"
        die "network '$network' is not defined in $SOROBAN_CONFIG.
Add a [[networks]] block for it (name, rpc_url, network_passphrase).
Known networks: ${known:-<none>}"
    fi
    ok "network '$network' resolved from .soroban/config.toml"
    ok "  rpc-url:    $RPC_URL"
    ok "  passphrase: (withheld; the operator does not need to see it)"
}

# ── Contract inventory ──────────────────────────────────────────────────────

# Deployable crates that the workspace actually builds.
#
# Read from the root Cargo.toml `members` list rather than hardcoded, because
# several `contracts/*` directories (campaign, donation, withdrawal,
# rate_limiter) are NOT workspace members and so are not produced by
# `cargo build --workspace`. Use `all_contract_crates` to see the full set.
workspace_contracts() {
    awk '
        # Start collecting at `members = [`.
        /members[[:space:]]*=[[:space:]]*\[/ { collecting = 1; next }
        # Stop at the closing bracket, which sits alone on its own line.
        collecting && /^[[:space:]]*\][[:space:]]*,?[[:space:]]*$/ { collecting = 0; next }
        collecting {
            gsub(/[]",[:space:]]/, " ")
            for (i = 1; i <= NF; i++) {
                if ($i ~ /^contracts\//) {
                    sub(/^contracts\//, "", $i)
                    print $i
                }
            }
        }
    ' "$REPO_ROOT/Cargo.toml" | sort -u
}

# Every `contracts/*` directory that exposes a Soroban entry point, workspace
# member or not.
all_contract_crates() {
    local dir
    for dir in "$REPO_ROOT"/contracts/*/; do
        [ -f "${dir}src/lib.rs" ] || continue
        if grep -q '^\[lib\]' "${dir}Cargo.toml" 2>/dev/null \
           && grep -q 'cdylib' "${dir}Cargo.toml" 2>/dev/null \
           && grep -q '#\[contract\]' "${dir}src/lib.rs" 2>/dev/null; then
            basename "$dir"
        fi
    done
}

wasm_path() {
    local name="$1"
    echo "${WASM_DIR:-$REPO_ROOT/target/wasm32-unknown-unknown/release}/$name.wasm"
}

# ── Mainnet approval gate ───────────────────────────────────────────────────
#
# Mainnet never runs by default. Three independent things must all be true:
#
#   1. The script was invoked with `--confirm-mainnet`.
#   2. `STELLAR_ALLOW_MAINNET` is exactly `1`.
#   3. `STELLAR_MAINNET_CONFIRM` contains the network passphrase verbatim, so the
#      operator has to have read it and typed it back. This mirrors a typed
#      "type the network name to confirm" prompt and cannot be satisfied by a
#      default, a wildcard, or an inherited value from CI.
#
# The passphrase is public (it is committed in .soroban/config.toml), so
# condition 3 is an intent check, not an authentication mechanism.

mainnet_approval_confirmed=0

require_mainnet_approval() {
    local flag_given="$1"
    local passphrase="$2"

    head1 "Mainnet approval gate"

    if [ "$flag_given" != "1" ]; then
        err "--confirm-mainnet was not passed."
        die "refusing to deploy to mainnet. Re-read docs/DEPLOYMENT.md, complete the
pre-deployment checklist, and re-run with --confirm-mainnet."
    fi
    ok "--confirm-mainnet was passed on the command line"

    if [ "${STELLAR_ALLOW_MAINNET:-}" != "1" ]; then
        err "STELLAR_ALLOW_MAINNET is not set to exactly 1 (current: ${STELLAR_ALLOW_MAINNET:-<unset>})."
        die "refusing to deploy to mainnet. Set STELLAR_ALLOW_MAINNET=1 only in the
session where you intend to deploy."
    fi
    ok "STELLAR_ALLOW_MAINNET=1"

    if [ -z "${STELLAR_MAINNET_CONFIRM:-}" ]; then
        err "STELLAR_MAINNET_CONFIRM is not set."
        die "refusing to deploy to mainnet. Re-type the mainnet passphrase from
.soroban/config.toml into STELLAR_MAINNET_CONFIRM to confirm intent."
    fi
    if [ "$STELLAR_MAINNET_CONFIRM" != "$passphrase" ]; then
        err "STELLAR_MAINNET_CONFIRM does not match the mainnet passphrase in
  $SOROBAN_CONFIG."
        die "refusing to deploy to mainnet."
    fi
    ok "STELLAR_MAINNET_CONFIRM matches the mainnet passphrase"

    warn "All three mainnet gates passed. This deploys real, unrecoverable value."
    mainnet_approval_confirmed=1
}

# Refuse to do anything irreversible unless the gate actually ran.
assert_mainnet_approved() {
    if [ "$mainnet_approval_confirmed" -ne 1 ]; then
        die "internal guard: mainnet deployment attempted without passing the approval gate."
    fi
}

# ── Soroban wrappers ────────────────────────────────────────────────────────
#
# `--source` is always a *reference* (an identity name registered with
# `soroban keys generate`, or an env-var reference), never a literal secret.
# When STELLAR_DEPLOYER_SECRET is set we hand the CLI the name of an
# environment variable instead of the value, so the value never appears in an
# argv.

deployer_source_ref() {
    if [ -n "${STELLAR_DEPLOYER_IDENTITY:-}" ]; then
        echo "$STELLAR_DEPLOYER_IDENTITY"
        return 0
    fi
    # `soroban contract deploy --source` accepts a secret-key or an identity
    # name. We prefer an identity; the value is only used if the operator has
    # deliberately exported it into the environment for this process.
    echo "STELLAR_DEPLOYER_SECRET"
}

soroban_configure_network() {
    local network="$1"
    if soroban network add "$network" \
        --rpc-url "$RPC_URL" \
        --network-passphrase "$PASSPHRASE" &>/dev/null; then
        ok "soroban network '$network' configured"
    else
        # `network add` also fails when the network already exists, which is not
        # an error. Say so rather than reporting a pass we cannot verify.
        warn "could not add network '$network' to the Soroban config.
  Continuing on the assumption that it is already configured. Verify with:
    soroban network list"
    fi
}
