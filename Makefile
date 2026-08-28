build:
	cargo build --target wasm32-unknown-unknown --release

test:
	cargo test

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

deploy-testnet:
	./scripts/deploy_testnet.sh

# ── Post-deployment validation ──────────────────────────────────────
validate:
	./scripts/verify_deployment.sh

validate-testnet:
	./scripts/verify_deployment.sh testnet

validate-config:
	./scripts/verify_config.sh

validate-contracts:
	./scripts/verify_contracts.sh

validate-operations:
	./scripts/verify_operations.sh

validate-cross-contract:
	./scripts/verify_cross_contract.sh

clean:
	cargo clean

.PHONY: build test fmt lint deploy-testnet clean validate validate-testnet validate-config validate-contracts validate-operations validate-cross-contract
