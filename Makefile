# Local commands mirror CI exactly — see .github/workflows/ci.yml.
#
# These flag sets are the single source of truth: the workflow runs the very
# same commands. Change the variables here and the workflow in the same PR.
CARGO_BUILD_FLAGS := build --workspace --target wasm32-unknown-unknown --release --locked
CARGO_TEST_FLAGS  := test --workspace --no-fail-fast --locked
CARGO_LINT_FLAGS  := clippy --workspace --all-targets --locked -- -D warnings

# Soroban's test host shares one ledger; CI pins this too.
export RUST_TEST_THREADS := 1

.PHONY: build test lint

build:
	cargo $(CARGO_BUILD_FLAGS)

test:
	cargo $(CARGO_TEST_FLAGS)

lint:
	cargo $(CARGO_LINT_FLAGS)
