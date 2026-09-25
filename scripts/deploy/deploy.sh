#!/usr/bin/env bash
# deploy.sh — shared contract deployment entry point (closes #708).
#
# Named `deploy.sh` rather than `deploy_testnet.sh` because it handles every
# network. `deploy_testnet.sh` is a thin wrapper that pins the network to
# testnet, and `deploy_mainnet.sh` is a thin wrapper that pins it to mainnet
# *and* adds the human-approval gate. Both wrappers delegate here, so there is
# exactly one deployment code path to review.
#
# Usage:
#   export STELLAR_DEPLOYER_SECRET=...          # injected from a secret manager
#   export STELLAR_DEPLOYER_IDENTITY=deployer   # optional local identity name
#   ./scripts/deploy/deploy.sh --network testnet \
#       [--contracts "escrow platform_config"] [--init] [--skip-build] [--dry-run]
#
# Flags:
#   --network <name>   Network to deploy to (default: testnet). Must be defined
#                      in .soroban/config.toml.
#   --contracts "a b"  Deploy only these crates. Values are crate directory
#                      names, which are also the WASM file names.
#                      Default: every workspace member contract.
#   --init             After deploying, print the per-contract initialisation
#                      reminder. Initialisation arguments are contract-specific
#                      and are never guessed.
#   --skip-build       Do not run cargo; assume the WASM is already built.
#   --dry-run          Print the plan and exit without submitting anything.
#   --confirm-mainnet  One of the three mainnet gates. Meaningless (and
#                      harmless) for other networks.
#
# MAINNET
#   Requesting `--network mainnet` does not deploy. It additionally requires
#   STELLAR_ALLOW_MAINNET=1 and STELLAR_MAINNET_CONFIRM=<mainnet passphrase>;
#   see lib.sh and deploy_mainnet.sh. This guard lives here as well as in the
#   wrapper so that calling this script directly cannot bypass it.
#
# CREDENTIALS
#   * Read from the environment only. Never from an argument, never from a file
#     in this repository, never generated automatically.
#   * Never printed, in whole or in part.
#   * A missing credential aborts the run before any build or transaction.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

NETWORK="testnet"
CONTRACT_OVERRIDE=""
DO_INIT=0
SKIP_BUILD=0
DRY_RUN=0
CONFIRM_MAINNET=0

# Keep the original argv so the credential guard can inspect it after parsing.
ORIGINAL_ARGS=("$@")
assert_no_secret_args ${ORIGINAL_ARGS[@]+"${ORIGINAL_ARGS[@]}"}

while [ $# -gt 0 ]; do
    case "$1" in
        --network)      NETWORK="${2:-}"; shift 2 ;;
        --network=*)    NETWORK="${1#*=}"; shift ;;
        --contracts)    CONTRACT_OVERRIDE="${2:-}"; shift 2 ;;
        --contracts=*)  CONTRACT_OVERRIDE="${1#*=}"; shift ;;
        --init)         DO_INIT=1; shift ;;
        --skip-build)   SKIP_BUILD=1; shift ;;
        --dry-run)      DRY_RUN=1; shift ;;
        --confirm-mainnet) CONFIRM_MAINNET=1; shift ;;
        --help|-h)      sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)              die "unknown argument '$1'. Try: $0 --help" ;;
    esac
done

# ── Plan ────────────────────────────────────────────────────────────────────

printf '╔══════════════════════════════════════════════════════════════╗\n'
printf '║  Contract deploy — StellarAid (%-8s)                   ║\n' "$NETWORK"
printf '╚══════════════════════════════════════════════════════════════╝\n'

head1 "Plan"
info "network:    $NETWORK"
info "contracts:  ${CONTRACT_OVERRIDE:-<all workspace member contracts>}"
info "initialise: $DO_INIT"
if [ "$SKIP_BUILD" -eq 1 ]; then
    info "build:      skipped"
else
    info "build:      cargo build --release --target wasm32-unknown-unknown --workspace"
fi
info "dry run:    $DRY_RUN"

resolve_network "$NETWORK"
require_secret_env STELLAR_DEPLOYER_SECRET "deploying to $NETWORK"
assert_repo_has_no_key_material

if [ "$NETWORK" = "mainnet" ]; then
    require_mainnet_approval "$CONFIRM_MAINNET" "$PASSPHRASE"
    assert_mainnet_approved
fi

# ── Contract selection ──────────────────────────────────────────────────────

if [ -n "$CONTRACT_OVERRIDE" ]; then
    CONTRACTS="$CONTRACT_OVERRIDE"
else
    CONTRACTS="$(workspace_contracts)"
fi
[ -n "$(printf '%s' "$CONTRACTS" | tr -d '[:space:]')" ] || die "no contracts selected"

for c in $CONTRACTS; do
    if [ ! -d "$REPO_ROOT/contracts/$c" ]; then
        die "unknown contract crate '$c' (no contracts/$c directory)"
    fi
done
info "selected: $(printf '%s ' $CONTRACTS)"

# ── Build ───────────────────────────────────────────────────────────────────

if [ "$SKIP_BUILD" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
    head1 "Build"
    # `--workspace` only covers crates registered in the root Cargo.toml.
    (cd "$REPO_ROOT" && cargo build --release --target wasm32-unknown-unknown --workspace)
    ok "workspace wasm build complete"
elif [ "$DRY_RUN" -eq 1 ] && [ "$SKIP_BUILD" -eq 0 ]; then
    head1 "Build"
    info "[dry run] would run: cargo build --release --target wasm32-unknown-unknown --workspace"
fi

for c in $CONTRACTS; do
    w="$(wasm_path "$c")"
    if [ ! -f "$w" ]; then
        die "missing WASM for '$c' at ${w#$REPO_ROOT/}.
Build it with:  cargo build -p $c --target wasm32-unknown-unknown --release
The workspace build does not produce WASM for crates that are not registered in
the root Cargo.toml [workspace] members list. See: ./scripts/deploy/preflight.sh"
    fi
done
ok "all selected WASM artefacts present"

# ── Network config ──────────────────────────────────────────────────────────

if [ "$DRY_RUN" -eq 0 ]; then
    require_cmd soroban "see docs/DEPLOYMENT.md"
    soroban_configure_network "$NETWORK"
fi

SOURCE_REF="$(deployer_source_ref)"
info "signing source reference: $SOURCE_REF (a reference, never a literal key)"

# ── Deploy ──────────────────────────────────────────────────────────────────

DEPLOYED=()

if [ "$DRY_RUN" -eq 1 ]; then
    head1 "Deploy (dry run)"
    for c in $CONTRACTS; do
        info "[dry run] would deploy: $c  ($(basename "$(wasm_path "$c")"))"
        DEPLOYED+=("$c=")
    done
else
    head1 "Deploy"
    for c in $CONTRACTS; do
        info "deploying $c ..."
        deploy_out=""
        if ! deploy_out="$(soroban contract deploy \
                --wasm "$(wasm_path "$c")" \
                --source "$SOURCE_REF" \
                --network "$NETWORK" 2>&1)"; then
            err "deploy failed for $c"
            printf '%s\n' "$deploy_out" >&2
            die "aborting. Ids already obtained in this run are listed below; do not
re-run the whole script blindly, it will create duplicate deployments."
        fi
        # The CLI prints the new contract id on its last stdout line. It is a
        # public identifier, safe to log.
        cid="$(printf '%s\n' "$deploy_out" | tail -n 1 | tr -d '[:space:]')"
        if [ -z "$cid" ]; then
            err "deploy for $c produced no contract id on stdout"
            printf '%s\n' "$deploy_out" >&2
            die "aborting. Inspect the output above before retrying."
        fi
        DEPLOYED+=("$c=$cid")
        ok "$c -> $cid"
    done
fi

# ── Initialisation reminder ─────────────────────────────────────────────────

if [ "$DO_INIT" -eq 1 ]; then
    head1 "Initialise"
    warn "initialisation arguments are contract-specific and are NEVER guessed"
    warn "by this script. Invoke each contract explicitly, e.g.:"
    warn "  soroban contract invoke --id \$ID --network $NETWORK --source \$SOURCE_REF -- \\"
    warn "    initialize --admin \$ADMIN_ADDRESS ..."
    info "scripts/deploy.sh (repo root) records the argument sets this repo has used"
    info "previously; docs/DEPLOYMENT.md §Initialisation explains each contract's"
    info "requirements. Initialising with a wrong admin is unrecoverable without a"
    info "migration, so this step is manual by design."
fi

# ── Record ids ──────────────────────────────────────────────────────────────

head1 "Record"

CONFIG_FILE="$REPO_ROOT/config/${NETWORK}_contracts.json"

if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry run] would record contract ids in ${CONFIG_FILE#$REPO_ROOT/}"
elif [ ! -f "$CONFIG_FILE" ]; then
    die "no config template at ${CONFIG_FILE#$REPO_ROOT/} to record ids into.
Copy config/testnet_contracts.json for a new network first. This script will not
invent a config file, and it will never invent a contract id."
else
    BACKUP="${CONFIG_FILE}.bak.$(date -u +%Y%m%dT%H%M%SZ)"
    cp "$CONFIG_FILE" "$BACKUP"
    info "previous config backed up to ${BACKUP#$REPO_ROOT/}"

    if ! command -v python3 &>/dev/null; then
        warn "python3 not available; record the ids by hand:"
        for entry in "${DEPLOYED[@]}"; do
            info "  \"${entry%%=*}\": { \"id\": \"${entry#*=}\", ... }"
        done
    else
        for entry in "${DEPLOYED[@]}"; do
            name="${entry%%=*}"
            cid="${entry#*=}"
            [ -n "$cid" ] || continue
            python3 - "$CONFIG_FILE" "$name" "$cid" <<'PY'
import json, sys
path, name, cid = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path) as fh:
    cfg = json.load(fh)
cfg.setdefault("contracts", {}).setdefault(name, {})["id"] = cid
with open(path, "w") as fh:
    json.dump(cfg, fh, indent=2)
    fh.write("\n")
PY
            ok "recorded $name id in ${CONFIG_FILE#$REPO_ROOT/}"
        done
    fi
fi

# ── Next steps ──────────────────────────────────────────────────────────────

head1 "Next"
info "1. Verify:    ./scripts/deploy/verify_deploy.sh $NETWORK"
info "2. Initialise every contract with deliberate arguments."
info "3. Smoke test one happy path per contract."
info "4. Commit only the config file. Never commit a secret."
info "5. Full runbook: docs/DEPLOYMENT.md"

if [ "$DRY_RUN" -eq 1 ]; then
    log ""
    ok "dry run complete — no transaction was submitted"
fi
