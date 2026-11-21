#!/usr/bin/env bash
# precommit_ts_lint.sh — eslint over sdk/bindings, for the pre-commit hook.
#
# Closes #886 (pre-commit hook support).
#
# Skips itself when Node or node_modules is absent. A contributor working
# purely in Rust should not be blocked by a missing toolchain for a directory
# they are not touching, and a hook that always fails gets bypassed
# permanently — which costs far more than the check is worth.
#
# What this does NOT do: install anything. `npm ci` in a git hook is
# surprising and slow, and the TypeScript CI workflow already proves the
# package installs cleanly from its lockfile. If node_modules is stale, the
# eslint you run locally is the eslint you get, and that is the point of a
# local hook.
#
# EXIT STATUS
#   0  clean, or not applicable
#   1  eslint reported problems

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINDINGS="$REPO_ROOT/sdk/bindings"

if [ ! -d "$BINDINGS" ]; then
    echo "precommit_ts_lint: sdk/bindings not present; skipping."
    exit 0
fi

if ! command -v node >/dev/null 2>&1; then
    echo "precommit_ts_lint: node not on PATH; skipping (CI covers this)."
    exit 0
fi

if ! command -v npm >/dev/null 2>&1; then
    echo "precommit_ts_lint: npm not on PATH; skipping (CI covers this)."
    exit 0
fi

if [ ! -d "$BINDINGS/node_modules" ]; then
    echo "precommit_ts_lint: sdk/bindings/node_modules not installed; skipping."
    echo "  Run: (cd sdk/bindings && npm ci)"
    exit 0
fi

echo "precommit_ts_lint: eslint sdk/bindings"
cd "$BINDINGS" || exit 0

if npm run --silent lint; then
    echo "precommit_ts_lint: clean"
    exit 0
fi

cat >&2 <<'EOF'

precommit_ts_lint: eslint reported problems.

  Fix them, auto-fix what is safe with:
      (cd sdk/bindings && npm run lint:fix)

  or bypass for this one commit:
      SKIP=ts-lint git commit -m "..."
EOF
exit 1
