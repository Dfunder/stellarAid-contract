# StellarAid contract workspace Makefile
#
# Local quality gates (mirrors CI expectations where applicable).
# `make check` is the single pre-push command: it runs non-mutating checks and
# never modifies the working tree.

CARGO ?= cargo

# Soroban's test host shares one ledger across tests; a parallel runner
# produces order-dependent failures. CI pins the same value (closes #1086).
export RUST_TEST_THREADS := 1

.DEFAULT_GOAL := check

# `build` uses the locked flag sets shared with CI (see the header note in
# CONTRIBUTING.md). Kept as a separate target so `check` stays non-mutating.
CARGO_BUILD_FLAGS := build --workspace --target wasm32-unknown-unknown --release --locked

.PHONY: check fmt fmt-check lint test build wasm-size deploy-testnet check-examples

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test:
	$(CARGO) test --workspace --no-fail-fast

# Composed from the same underlying targets/commands to keep local and CI in sync.
check: fmt-check lint test

build:
	$(CARGO) $(CARGO_BUILD_FLAGS)

# Print the size of every built contract WASM (#1118). Build first with
# `make build` (or `cargo build --release --target wasm32-unknown-unknown`).
wasm-size:
	@ls -l target/wasm32-unknown-unknown/release/*.wasm | awk '{print $$5, $$9}'

# Single deployment entry point (closes #1090). The removed root-level
# scripts/deploy_testnet.sh is not referenced anywhere; this target and the
# docs point at the maintained scripts/deploy/ tree.
ARGS ?=
deploy-testnet:
	./scripts/deploy/deploy_testnet.sh $(ARGS)

# Standalone crate check (closes #1092): verifies the examples stay on the
# workspace's stellar-strkey major with their own committed lockfile.
check-examples:
	cargo check --manifest-path examples/Cargo.toml --locked
