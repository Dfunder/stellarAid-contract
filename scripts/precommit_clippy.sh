#!/usr/bin/env bash
# precommit_clippy.sh — clippy over the crates touched by the current commit.
#
# Closes #886 (pre-commit hook support).
#
# This is a NEW script, deliberately separate from scripts/clippy_check.sh.
# That one runs the whole workspace (tracked as #792); this one is the fast
# pre-commit approximation and has different scope semantics. They must not be
# merged, and fixing one must not silently change the other.
#
# WHY NOT `cargo clippy --workspace`
#   On this workspace that is minutes, warm, across 29 crates. A pre-commit
#   hook that takes minutes is one that gets bypassed with --no-verify within
#   a fortnight, at which point it protects nothing. So: resolve the staged
#   .rs files to their owning crates and run clippy over just those. The lint
#   set is identical (`--all-targets -- -D warnings`); only the scope is
#   narrower. CI still runs the full workspace pass, which is the real gate.
#
# SCOPE RULES
#   contracts/<name>/src/**  -> crate <name>
#   sdk/src/**               -> crate `sdk`
#   tests/<name>/src/**      -> crate <name>
#   anything else            -> no crate; skipped
#
#   `shared` is always added when any contract is in scope. A change to
#   contracts/shared/src/pause.rs can break every contract that depends on it
#   and the hook cannot know which, so it checks `shared` itself and lets CI
#   catch the downstream.
#
# USAGE
#   scripts/precommit_clippy.sh              # inspect the git index
#   scripts/precommit_clippy.sh <crate>...   # check specific crates
#
# EXIT STATUS
#   0  clean, or nothing in scope
#   1  clippy reported a problem in at least one crate
#   2  not a git checkout, or cargo is unavailable

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo "precommit_clippy: not inside a git checkout; run from the repo root." >&2
    exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "precommit_clippy: cargo is not on PATH — https://rustup.rs" >&2
    exit 2
fi

# Crates that exist on disk but are deliberately not registered in the root
# [workspace] members list — see the NOTE in the root Cargo.toml. `cargo
# clippy -p` cannot build these, so asking is a guaranteed failure that
# teaches the author to bypass the hook.
NOT_MEMBERS="campaign donation withdrawal worker"

# Resolve a repo-relative path to its owning crate name, or fail.
crate_for_path() {
    case "$1" in
        contracts/*) echo "$1" | cut -d/ -f2 ;;
        sdk/src/*)   echo "sdk" ;;
        tests/*)     echo "$1" | cut -d/ -f2 ;;
        *)           return 1 ;;
    esac
    return 0
}

# ── Collect crates ──────────────────────────────────────────────────────────

RAW=()
if [ "$#" -gt 0 ]; then
    RAW=("$@")
else
    # ACM = Added / Copied / Modified; a deleted file has nothing to lint.
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        if c="$(crate_for_path "$f")"; then
            RAW+=("$c")
        fi
    done < <(git diff --cached --name-only --diff-filter=ACM -- '*.rs')
fi

# Deduplicate, preserving order.
CRATES=()
for c in ${RAW[@]+"${RAW[@]}"}; do
    [ -n "$c" ] || continue
    seen=0
    for u in ${CRATES[@]+"${CRATES[@]}"}; do
        if [ "$u" = "$c" ]; then seen=1; break; fi
    done
    [ "$seen" -eq 0 ] && CRATES+=("$c")
done

if [ "${#CRATES[@]}" -eq 0 ]; then
    echo "precommit_clippy: no staged Rust changes; nothing to lint."
    exit 0
fi

# Drop crates outside the workspace, announcing each one. Silently skipping
# them would look like they were covered.
CHECK=()
for c in "${CRATES[@]}"; do
    is_non_member=0
    for nm in $NOT_MEMBERS; do
        if [ "$nm" = "$c" ]; then
            echo "precommit_clippy: skipping '$c' — not a [workspace] member (see the root Cargo.toml)."
            is_non_member=1
            break
        fi
    done
    [ "$is_non_member" -eq 0 ] && CHECK+=("$c")
done

# `shared` affects every contract that depends on it.
if [ "${#CHECK[@]}" -gt 0 ]; then
    has_shared=0
    for c in "${CHECK[@]}"; do
        [ "$c" = "shared" ] && { has_shared=1; break; }
    done
    if [ "$has_shared" -eq 0 ]; then
        CHECK+=("shared")
    fi
fi

if [ "${#CHECK[@]}" -eq 0 ]; then
    echo "precommit_clippy: every crate in scope is outside the workspace; nothing to lint."
    exit 0
fi

echo "precommit_clippy: checking ${#CHECK[@]} crate(s): ${CHECK[*]}"

status=0
for c in "${CHECK[@]}"; do
    if cargo clippy --package "$c" --all-targets -- -D warnings; then
        echo "precommit_clippy: $c clean"
    else
        echo "precommit_clippy: $c reported problems" >&2
        status=1
    fi
done

if [ "$status" -ne 0 ]; then
    cat >&2 <<'EOF'

precommit_clippy: failed.

  Fix the warnings, or bypass this hook for this one commit:
      SKIP=clippy git commit -m "..."

  CI runs `cargo clippy --workspace --all-targets -- -D warnings`, so anything
  skipped here will fail the pull request.
EOF
fi

exit "$status"
