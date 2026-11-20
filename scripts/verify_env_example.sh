#!/usr/bin/env bash
# Fail when an environment variable is used without appearing in the canonical
# reference, or when .env.example drifts from that reference.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reference="$repo_root/docs/ENV_REFERENCE.md"
example="$repo_root/.env.example"

for file in "$reference" "$example"; do
    if [ ! -f "$file" ]; then
        printf 'error: missing required file %s\n' "$file" >&2
        exit 1
    fi
done

pattern='(STELLAR|SOROBAN)_[A-Z0-9_]+|NO_COLOR|ADMIN_SECRET|WASM_DIR|DEPLOYMENT_MANIFEST|NETWORK'

documented="$(grep -oE "\`${pattern}\`" "$reference" | tr -d '`' | sort -u)"

used="$(
    {
        grep -RhoE "$pattern" "$repo_root" \
            --include='*.rs' \
            --include='*.sh' \
            --include='*.md' \
            --include='*.yml' \
            --include='*.yaml' \
            --include='*.toml' \
            --exclude-dir=.git \
            --exclude-dir=target \
            --exclude='verify_env_example.sh' || true
        grep -hoE "$pattern" "$example" || true
    } | sort -u
)"

failed=0

check_documented() {
    local var="$1"
    if ! printf '%s\n' "$documented" | grep -qx -- "$var"; then
        printf 'error: %s is used but is not documented in docs/ENV_REFERENCE.md\n' "$var" >&2
        failed=1
    fi
}

while IFS= read -r var; do
    [ -n "$var" ] || continue
    check_documented "$var"
done <<< "$used"

while IFS= read -r line; do
    if [[ "$line" =~ ^([A-Z][A-Z0-9_]*)= ]]; then
        check_documented "${BASH_REMATCH[1]}"
    fi
done < "$example"

if [ "$failed" -ne 0 ]; then
    exit 1
fi

printf 'env reference is in sync (%s variables documented)\n' \
    "$(printf '%s\n' "$documented" | grep -c .)"
