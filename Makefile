.PHONY: deploy-testnet check-examples

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
