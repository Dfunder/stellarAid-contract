#!/usr/bin/env bash
# scripts/deploy.sh — thin compatibility wrapper (closes #1129).
#
# The old root script had an unterminated `if`, unreachable code after
# `continue`, unquoted variables, a swallowed config failure (`|| true`) and
# passed the admin secret on the command line (`--source "$ADMIN_SECRET"`),
# which exposes it in `ps` output.
#
# All deployment logic now lives in scripts/deploy/deploy.sh, which resolves
# the signer through an identity reference (STELLAR_DEPLOYER_IDENTITY /
# STELLAR_DEPLOYER_SECRET from the environment) and never puts a secret in argv.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -n "${ADMIN_SECRET:-}" && -z "${STELLAR_DEPLOYER_SECRET:-}" ]]; then
  # Map the legacy variable through the environment only; never as an argument.
  export STELLAR_DEPLOYER_SECRET="$ADMIN_SECRET"
fi
unset ADMIN_SECRET

exec "$SCRIPT_DIR/deploy/deploy.sh" "$@"
