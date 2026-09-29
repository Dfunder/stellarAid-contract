#!/usr/bin/env bash
# precommit_fmt.sh — rustfmt the staged .rs files, for the pre-commit hook.
#
# Closes #886 (pre-commit hook support).
#
# WHY A LOCAL HOOK AND NOT THE pre-commit-hooks `rustfmt` HOOK
#   There isn't one to use. The `rustfmt` hook was removed from
#   pre-commit/pre-commit-hooks, and the community replacements all work by
#   creating a throwaway Rust environment to install rustfmt into — which
#   means a second copy of the formatter alongside the one rust-toolchain.toml
#   already installs, and a hook that fails on a machine with no network. This
#   script calls the `rustfmt` that is already on PATH, so the hook, the
#   Docker image and CI all format with the identical binary.
#
# WHY STAGED FILES AND NOT `cargo fmt --all`
#   `cargo fmt --all` rewrites the whole workspace, so an unrelated
#   unformatted file elsewhere in the tree would silently join your commit.
#   That is how a formatting change ends up touching 60 files in a PR about
#   something else. Only what you staged is touched.
#
# On failure the changed files are re-staged, so `git commit` proceeds with
# the formatted version rather than failing and leaving the author to guess.
#
# EXIT STATUS
#   0  everything staged is already formatted, or there was nothing to do
#   2  rustfmt is unavailable
#   3  rustfmt failed (a syntax error, most likely)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v rustfmt >/dev/null 2>&1; then
    echo "precommit_fmt: rustfmt is not on PATH." >&2
    echo "  rustup component add rustfmt   # or: rustup show" >&2
    exit 2
fi

# ACM = Added / Copied / Modified; a deleted file has nothing to format.
STAGED=()
while IFS= read -r f; do
    [ -n "$f" ] || continue
    [ -f "$f" ] || continue
    STAGED+=("$f")
done < <(git diff --cached --name-only --diff-filter=ACM -- '*.rs')

if [ "${#STAGED[@]}" -eq 0 ]; then
    echo "precommit_fmt: no staged Rust files."
    exit 0
fi

# The edition comes from the workspace Cargo.toml, not from a literal here, so
# this cannot drift from what the crates actually declare.
EDITION="$(awk '
    /^\[workspace\.package\]/ { in_ws = 1; next }
    /^\[/                { in_ws = 0 }
    in_ws && /^[[:space:]]*edition[[:space:]]*=/ {
        sub(/^[^=]*=[[:space:]]*/, "")
        gsub(/["[:space:]]/, "")
        print; exit
    }
' Cargo.toml 2>/dev/null || true)"

if [ -z "$EDITION" ]; then
    echo "precommit_fmt: could not read workspace edition from Cargo.toml; defaulting to 2021."
    EDITION=2021
fi

echo "precommit_fmt: rustfmt (edition $EDITION) on ${#STAGED[@]} file(s)"

changed=()
for f in "${STAGED[@]}"; do
    # rustfmt exits non-zero on a parse error, and may still have rewritten
    # earlier files. Report which file failed.
    if ! rustfmt --edition "$EDITION" "$f"; then
        echo >&2
        echo "precommit_fmt: rustfmt failed on $f" >&2
        if [ "$f" != "${STAGED[0]}" ]; then
            for c in ${changed[@]+"${changed[@]}"}; do
                git add -- "$c"
            done
            echo "precommit_fmt: re-staged the files that did format successfully:" >&2
            for c in ${changed[@]+"${changed[@]}"}; do
                echo "  $c" >&2
            done
            echo >&2
        fi
        echo "  A parse error here means the file does not compile. Fix that first." >&2
        echo "  To commit unformatted:  SKIP=fmt git commit -m \"...\"" >&2
        exit 3
    fi
    if ! git diff --quiet -- "$f"; then
        changed+=("$f")
    fi
done

if [ "${#changed[@]}" -eq 0 ]; then
    echo "precommit_fmt: already formatted."
    exit 0
fi

for c in "${changed[@]}"; do
    git add -- "$c"
done

echo
echo "precommit_fmt: reformatted and re-staged ${#changed[@]} file(s):"
for c in "${changed[@]}"; do
    echo "  $c"
done
echo
echo "  These are staged and will be part of this commit. Unstage any you"
echo "  did not mean to change."
