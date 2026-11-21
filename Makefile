# StellarAid contract workspace Makefile
#
# Local quality gates (mirrors CI expectations where applicable).
# `make check` is the single pre-push command: it runs non-mutating checks and
# never modifies the working tree.

CARGO ?= cargo

.DEFAULT_GOAL := check

.PHONY: check fmt fmt-check lint test

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
