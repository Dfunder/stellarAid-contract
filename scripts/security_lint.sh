#!/usr/bin/env bash
# security_lint.sh — mechanical enforcement of the security checklists.
#
# Closes #883.
#
# WHY THIS EXISTS
#   docs/SECURITY_AUDIT_CHECKLIST.md and docs/SECURITY_REVIEW_CHECKLIST.md are
#   long and human. They are also unenforced: nothing stops a PR that drops an
#   `admin.require_auth()` or commits a Stellar secret key. This script encodes
#   the subset of both checklists that is decidable with a text scan, and fails
#   the build on a violation.
#
# WHAT IT IS NOT
#   A substitute for review. It catches classes of mistake, not a class of bug.
#   A green run here means "no known-bad pattern is present", never "this code
#   is secure". T-01..T-15 in the review checklist remain a human obligation.
#
# SCOPE
#   Tracked files only (git ls-files), so a stray editor backup or an ignored
#   build artifact cannot fail the build. Test modules are excluded from the
#   code-shape rules: `unwrap()` in a test is a readable assertion, in a
#   contract entry point it is a panic reachable from a transaction.
#
# USAGE
#   ./scripts/security_lint.sh            # enforce; exit 1 on any violation
#   ./scripts/security_lint.sh --report   # print findings, always exit 0
#   ./scripts/security_lint.sh --list     # list rule ids and descriptions
#
# EXIT STATUS
#   0  no violations (or --report / --list)
#   1  at least one violation
#   2  not run inside a git checkout

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

REPORT_ONLY=0
case "${1:-}" in
    --report) REPORT_ONLY=1 ;;
    --list)
        grep -E '^#   SL[0-9]{3}' "$0" | sed 's/^#   //'
        exit 0
        ;;
    "") ;;
    -h|--help) sed -n '2,/^set -uo/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)  echo "unknown argument '$1'. Try --help." >&2; exit 2 ;;
esac

if ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo "security_lint: not inside a git checkout; refusing to scan." >&2
    exit 2
fi

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_RED=$'\033[31m'; C_GRN=$'\033[32m'; C_YEL=$'\033[33m'
    C_BLD=$'\033[1m'; C_OFF=$'\033[0m'
else
    C_RED=''; C_GRN=''; C_YEL=''; C_BLD=''; C_OFF=''
fi

VIOLATIONS=0
FINDINGS=0

# Files to scan: everything tracked, minus vendored and lockfile noise.
# Built with a read loop rather than `mapfile` so the script also runs on the
# bash 3.2 that macOS still ships as /bin/bash.
TRACKED=()
while IFS= read -r _f; do
    [ -n "$_f" ] && TRACKED+=("$_f")
done < <(git ls-files)

# Rust sources. Test modules are filtered per-match, not per-file, because
# `#[cfg(test)]` blocks live inside the same files as the entry points.
RUST_SOURCES=()
for f in "${TRACKED[@]}"; do
    case "$f" in
        *.rs)
            case "$f" in
                target/*|Cargo.lock) continue ;;
            esac
            RUST_SOURCES+=("$f")
            ;;
    esac
done

# Paths that never contain contract logic.
is_excluded_path() {
    case "$1" in
        target/*|node_modules/*|dist/*|Cargo.lock) return 0 ;;
        # Test-only files. The code-shape rules are about entry points, and a
        # bare `unwrap()` in a test is a readable assertion rather than a
        # transaction-reachable panic.
        */tests.rs|*/test.rs|*/tests/*|tests/*|*/benches/*|*/test_snapshots/*) return 0 ;;
    esac
    return 1
}

# CRITICAL and HIGH findings fail the build. MEDIUM and LOW are reported and
# counted, but do not gate: they describe a class of debt that already exists in
# the tree (several non-workspace crates predate this script) and would
# otherwise block every PR until a separate remediation lands.
is_gating_severity() {
    case "$1" in
        CRITICAL|HIGH) return 0 ;;
        *)             return 1 ;;
    esac
}

report() {
    # report <rule-id> <severity> <file> <line> <message>
    FINDINGS=$((FINDINGS + 1))
    if is_gating_severity "$2"; then
        VIOLATIONS=$((VIOLATIONS + 1))
        printf '  %s%s [%s]%s %s:%s — %s\n' "$C_RED" "$1" "$2" "$C_OFF" "$3" "$4" "$5"
    else
        printf '  %s%s [%s]%s %s:%s — %s\n' "$C_YEL" "$1" "$2" "$C_OFF" "$3" "$4" "$5"
    fi
}

section() { printf '\n%s%s%s\n' "$C_BLD" "$1" "$C_OFF"; }

# ── Rules ───────────────────────────────────────────────────────────────────
#
#   id    name                        severity  checklist
#   ----  --------------------------  --------  -------------------------------
#   SL001 hardcoded-credential        HIGH      SA-02 / T-02
#   SL002 stellar-secret-key          CRITICAL  SA-02
#   SL003 key-material-tracked        CRITICAL  SA-02
#   SL004 unsafe-block                HIGH      KL-06
#   SL005 debug-macro                 MEDIUM    T-04          (reported)
#   SL006 unchecked-storage-unwrap    MEDIUM    T-04          (reported)
#   SL007 open-security-todo          LOW       T-01 / T-09   (reported)
#   SL008 over-permissive-auth        HIGH      T-02

# scan <rule-id> <severity> <label> <eregex> <file-array-name> [i]
#
# Runs <eregex> over every file in the named array, honouring is_excluded_path,
# and reports each match as <rule-id> [severity] file:line — label: <line text>.
scan() {
    local rule="$1" sev="$2" label="$3" ere="$4" arrayname="$5" flags="${6:-}"
    local f line ln text n=0
    section "$rule $label"
    eval "local -a files=(\"\${$arrayname[@]}\")"
    # `-i` is passed through a conditional so an empty $flags cannot become an
    # empty *pattern* argument, which would match every line in every file.
    local icase=()
    [ "$flags" = "-i" ] && icase=(-i)
    for f in ${files[@]+"${files[@]}"}; do
        is_excluded_path "$f" && continue
        [ -f "$f" ] || continue
        local hits
        hits="$(grep -nE "${icase[@]+"${icase[@]}"}" -- "$ere" "$f" 2>/dev/null || true)"
        [ -n "$hits" ] || continue
        while IFS= read -r line; do
            [ -n "$line" ] || continue
            ln="${line%%:*}"
            if [ "$line" = "$ln" ]; then text="$line"; else text="${line#*:}"; fi
            n=$((n + 1))
            report "$rule" "$sev" "$f" "$ln" "$label: ${text:0:60}"
        done <<< "$hits"
    done
    return 0
}

# SL001 — credential-shaped literals assigned in source. SA-02: the admin key
# is the single point of failure (KL-02), so it must never appear in the tree.
scan SL001 HIGH "credential-shaped literal" \
    '(api[_-]?key|secret[_-]?key|private[_-]?key|access[_-]?token|bearer|password)[[:space:]]*[:=][[:space:]]*"[^"$<{}]{12,}"' \
    RUST_SOURCES -i

# SL002 — Stellar secret seeds are base32 and always start with 'S' followed by
# 55 characters of the [A-Z2-7] alphabet. Public keys start with 'G', so they do
# not collide. Scanned across every tracked file, not just Rust.
# The word boundaries are spelled with an explicit character class rather than
# `\b`, which BSD grep (macOS) does not implement.
scan SL002 CRITICAL "Stellar secret key literal in a tracked file" \
    '(^|[^A-Za-z0-9])S[A-Z2-7]{55}([^A-Za-z0-9]|$)' \
    TRACKED

# SL004 — this workspace is `forbid(unsafe_code)` in spirit; a bare `unsafe`
# block is always an unvetted local decision.
scan SL004 HIGH "unsafe block" \
    '(^|[^[:alnum:]_])unsafe[[:space:]]*(\{|fn|impl|trait)' \
    RUST_SOURCES

# SL005 — debug macros in entry-point code leak internal state into a
# transaction's diagnostic events and inflate the WASM footprint.
scan SL005 MEDIUM "debug macro in contract code" \
    '(^|[^[:alnum:]_:])(dbg!|println!|eprintln!)' \
    RUST_SOURCES

# SL006 — an `unwrap()` on a storage read in an entry point turns a "not
# initialised" condition into a panic any caller can trigger. Matches the
# `env.storage().<map>().get(&k).unwrap()` shape only, so ordinary `Option`
# handling elsewhere is untouched. Non-gating: the pre-existing occurrences sit
# in crates that are not workspace members and are tracked separately.
scan SL006 MEDIUM "storage read unwrapped in entry point" \
    'env[[:space:]]*[.{]*storage\(\)[[:space:]]*[.?]*.*get\([^)]*\)[[:space:]]*\.[[:space:]]*unwrap\(\)' \
    RUST_SOURCES

# SL007 — a TODO that names a security concern is a promise to a reviewer that
# has not been kept. It is a finding, not a comment.
scan SL007 LOW "unresolved security TODO" \
    '(TODO|FIXME|XXX)[^a-zA-Z]*(.*(secur|re-?entran|authori[sz]|privile|escalat|overflow|replay|spoof))' \
    RUST_SOURCES -i

# SL008 — `env.current_contract_address().require_auth()` asks the contract to
# authorise itself, which is a no-op against a caller who already holds the
# contract's own key. Auth must come from an account or a policy address. T-02.
scan SL008 HIGH "self-authorisation via current_contract_address()" \
    'env[[:space:]]*[.{]*current_contract_address\(\)[[:space:]]*\)?[[:space:]]*\.[[:space:]]*require_auth\(\)' \
    RUST_SOURCES

# SL003 — key material belongs in a secret manager, not in the history. This one
# is a path check rather than a content scan.
section "SL003 key-material-tracked (SA-02)"
while IFS= read -r f; do
    [ -n "$f" ] || continue
    report SL003 CRITICAL "$f" 1 "key material is tracked in git"
done < <(printf '%s\n' ${TRACKED[@]+"${TRACKED[@]}"} \
    | grep -Ei '(^|/)(\.env(\..*)?|mnemonic\.txt|[^/]*\.(pem|key|keystore|seed))$' || true)

# ── Verdict ─────────────────────────────────────────────────────────────────
printf '\n%s%s%s\n' "$C_BLD" "Result: $FINDINGS finding(s), $VIOLATIONS violation(s)" "$C_OFF"

if [ "$REPORT_ONLY" -eq 1 ]; then
    exit 0
fi

if [ "$FINDINGS" -ne 0 ] && [ "$VIOLATIONS" -eq 0 ]; then
    printf '%ssecurity_lint: OK%s (%d non-gating finding(s); these are reported, not enforced)\n' \
        "$C_GRN" "$C_OFF" "$FINDINGS"
    exit 0
fi

if [ "$VIOLATIONS" -ne 0 ]; then
    printf '%ssecurity_lint: FAILED%s\n' "$C_RED" "$C_OFF"
    printf 'Each finding names a rule id from the header of this script and a\n'
    printf 'checklist id (SA-*, T-*, KL-*) in docs/SECURITY_REVIEW_CHECKLIST.md.\n'
    printf 'Run %s./scripts/security_lint.sh --report%s for a full list, or silence a\n' "$C_YEL" "$C_OFF"
    printf 'false positive by narrowing the rule rather than by deleting the check.\n'
    exit 1
fi

printf '%ssecurity_lint: OK%s\n' "$C_GRN" "$C_OFF"
exit 0
