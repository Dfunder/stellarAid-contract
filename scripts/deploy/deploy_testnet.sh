#!/usr/bin/env bash
# deploy_testnet.sh — testnet deployment (closes #708).
#
# Thin wrapper that pins the network to testnet and delegates to deploy.sh.
# All of the logic lives there, so this file is deliberately boring.
#
# Usage:
#   export STELLAR_DEPLOYER_SECRET=...          # injected from a secret manager
#   export STELLAR_DEPLOYER_IDENTITY=deployer   # optional local identity name
#   ./scripts/deploy/deploy_testnet.sh [--contracts "escrow platform_config"] \
#                                     [--init] [--skip-build] [--dry-run]
#
# Safe to rehearse: run it with --dry-run first. It prints the plan and
# submits nothing, and it still refuses to start without a credential present
# in the environment, so a missing secret surfaces before anything is built.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

assert_no_secret_args "$@"

head1 "Testnet deployment"

info "All logic is in scripts/deploy/deploy.sh. This wrapper only pins the"
info "network to testnet. Recommended first run:  --dry-run"

# `--network testnet` is placed first so an explicit `--network mainnet` in the
# forwarded arguments still wins in deploy.sh's last-one-wins parsing only if it
# appears later; to remove all doubt we reject it here instead of relying on
# argument order.
for arg in "$@"; do
    case "$arg" in
        --network|--network=*)
            die "use scripts/deploy/deploy_mainnet.sh for mainnet. This script is
testnet-only by construction, so that a mistyped --network cannot reach the
mainnet path from here."
            ;;
    esac
done

exec bash "$SCRIPT_DIR/deploy.sh" --network testnet "$@"
