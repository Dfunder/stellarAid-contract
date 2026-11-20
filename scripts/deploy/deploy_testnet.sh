#!/usr/bin/env bash
# Canonical testnet deployment entry point (closes #1090).
#
# This is the only deploy_testnet.sh in the repository. The old root-level
# scripts/deploy_testnet.sh generated a fresh key on every run, deployed two
# contracts and wrote an untracked .env.contracts; it has been removed. All
# credential handling lives in lib.sh and all deployment logic in deploy.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

assert_no_secret_args "$@"

# This script is testnet-only by construction; a mistyped --network must never
# reach the mainnet path from here.
for arg in "$@"; do
    case "$arg" in
        --network|--network=*)
            die "this wrapper is testnet-only; do not pass --network."
            ;;
    esac
done

head1 "Testnet deployment"
require_secret_env STELLAR_DEPLOYER_SECRET "testnet deployment"

if [ ! -f "$SCRIPT_DIR/deploy.sh" ]; then
    die "scripts/deploy/deploy.sh is missing from this tree. Restore the shared deployment runner before deploying."
fi

exec bash "$SCRIPT_DIR/deploy.sh" --network testnet "$@"
