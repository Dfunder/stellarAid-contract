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

# ── Containerised build / test (closes #708) ──────────────────────────────
docker-build:
	docker build -t stellaraid-contracts .

docker-test:
	docker build --target test -t stelleraid-contracts:test .

# ── Supply-chain (closes #879, #880) ──────────────────────────────────────
deny:
	CARGO_HOME=${CARGO_HOME:-/var/folders/st/r29n457x5r5dpnxqr7xz5hbc0000gn/T/opencode/cargo-home} PATH="${CARGO_HOME}/bin:${PATH}" cargo deny check

audit:
	CARGO_HOME=${CARGO_HOME:-/var/folders/st/r29n457x5r5dpnxqr7xz5hbc0000gn/T/opencode/cargo-home} PATH="${CARGO_HOME}/bin:${PATH}" cargo audit

# ── Deployment pipeline (closes #708) ─────────────────────────────────────
# Wrappers around scripts/deploy/. These read credentials from the
# environment; see docs/DEPLOYMENT.md §7.
preflight:
	./scripts/deploy/preflight.sh testnet

deploy-dry-run:
	./scripts/deploy/deploy_testnet.sh --dry-run

deploy-verify:
	./scripts/deploy/verify_deploy.sh testnet

# Refuses to run without all three mainnet gates. See docs/DEPLOYMENT.md §5.
deploy-mainnet:
	./scripts/deploy/deploy_mainnet.sh --confirm-mainnet

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

.PHONY: build test fmt lint deny audit deploy-testnet clean validate validate-testnet validate-config validate-contracts validate-operations validate-cross-contract docker-build docker-test preflight deploy-dry-run deploy-verify deploy-mainnet
