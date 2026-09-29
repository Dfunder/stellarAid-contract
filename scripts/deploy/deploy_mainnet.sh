#!/usr/bin/env bash
# deploy_mainnet.sh — mainnet deployment, gated behind explicit human approval.
#
# This script performs no deployment logic of its own. It enforces the
# mainnet-only requirements that do not belong in a general deploy path, then
# delegates to scripts/deploy/deploy.sh with the network pinned to mainnet.
#
# ═══════════════════════════════════════════════════════════════════════════
#  MAINNET GATE — all three must hold, or this script exits without submitting
#  anything:
#
#    1. --confirm-mainnet passed on the command line
#    2. STELLAR_ALLOW_MAINNET=1 in the environment
#    3. STELLAR_MAINNET_CONFIRM set to the mainnet network passphrase, verbatim,
#       copied out of .soroban/config.toml by the operator
#
#  Condition 3 is the interactive-style confirmation: the operator has to have
#  read the passphrase and typed it back. There is no default, no wildcard, and
#  no "yes" shortcut. The passphrase is public, so this proves intent, not
#  identity — authorisation comes from the signing key, which is read from
#  STELLAR_DEPLOYER_SECRET and never printed.
#
#  A CI job that has not been given all three fails closed. That is the
#  intended behaviour, not a bug to work around.
#
#  The same three conditions are re-checked inside deploy.sh, so calling that
#  script directly with --network mainnet is gated too.
# ═══════════════════════════════════════════════════════════════════════════
#
# Usage:
#   export STELLAR_DEPLOYER_SECRET=...
#   export STELLAR_DEPLOYER_IDENTITY=mainnet-deployer
#   export STELLAR_ALLOW_MAINNET=1
#   export STELLAR_MAINNET_CONFIRM='<passphrase from .soroban/config.toml>'
#   export MAINNET_OPERATOR='<name>' SECOND_APPROVER='<name>'
#   export ROLLBACK_CONTRACT_IDS='platform_config=C... escrow=C...'
#   export CHANGELOG_REF="$(git rev-parse HEAD)"
#   ./scripts/deploy/deploy_mainnet.sh --confirm-mainnet \
#       --contracts "platform_config escrow" --init
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

assert_no_secret_args "$@"

FORWARDED=()
CONFIRM_MAINNET=0
while [ $# -gt 0 ]; do
    case "$1" in
        --confirm-mainnet) CONFIRM_MAINNET=1; shift ;;
        --network|--network=*)
            die "this script is mainnet-only; drop --network."
            ;;
        *)                 FORWARDED+=("$1"); shift ;;
    esac
done

printf '╔══════════════════════════════════════════════════════════════╗\n'
printf '║  MAINNET DEPLOYMENT — StellarAid                             ║\n'
printf '╚══════════════════════════════════════════════════════════════╝\n'

# Resolve the network first so the confirmation token is compared against the
# real value. Nothing has been submitted at this point.
resolve_network mainnet

# ── The gate ────────────────────────────────────────────────────────────────
require_mainnet_approval "$CONFIRM_MAINNET" "$PASSPHRASE"

# ── Additional mainnet-only requirements ────────────────────────────────────

head1 "Mainnet requirements"

if [ -z "${MAINNET_OPERATOR:-}" ] || [ -z "${SECOND_APPROVER:-}" ] \
   || [ "$MAINNET_OPERATOR" = "$SECOND_APPROVER" ]; then
    err "MAINNET_OPERATOR and SECOND_APPROVER must name two different humans."
    die "refusing to deploy to mainnet. A single-operator mainnet deploy is not
permitted. Record both names in the change ticket and export them for this run
so the run log carries them."
fi
ok "two distinct approvers recorded: $MAINNET_OPERATOR and $SECOND_APPROVER"

if [ -d "$REPO_ROOT/backups" ]; then
    ok "backups/ exists (a state snapshot was taken — MAINTENANCE_WINDOWS.md §3)"
else
    warn "no backups/ directory. Take a state snapshot before deploying."
fi

if [ -z "${ROLLBACK_CONTRACT_IDS:-}" ]; then
    err "ROLLBACK_CONTRACT_IDS is not set."
    die "refusing to deploy to mainnet without a recorded rollback target.
    export ROLLBACK_CONTRACT_IDS='platform_config=C... escrow=C...'
See docs/DEPLOYMENT.md §Rollback and docs/UPGRADE_AND_ROLLBACK.md."
fi
ok "ROLLBACK_CONTRACT_IDS is set (previous deployment recorded)"

if [ -z "${CHANGELOG_REF:-}" ]; then
    err "CHANGELOG_REF is not set."
    die "refusing to deploy to mainnet without an auditable reference to the
deployed revision, e.g.  export CHANGELOG_REF=\"\$(git rev-parse HEAD)\"."
fi
ok "CHANGELOG_REF is set: $CHANGELOG_REF"

# ── Delegate ────────────────────────────────────────────────────────────────

assert_mainnet_approved

head1 "Delegating to the shared deploy path"
info "network is pinned to mainnet. All other flags are forwarded unchanged."
info "Add --dry-run first if you have not already rehearsed this exact plan."

exec bash "$SCRIPT_DIR/deploy.sh" --network mainnet --confirm-mainnet \
    ${FORWARDED[@]+"${FORWARDED[@]}"}
