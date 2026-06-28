build:
	cargo build --target wasm32-unknown-unknown --release

test:
	cargo test

bindings: bindings-rust bindings-typescript

bindings-rust:
	soroban contract bindings rust --contract-id $$(cat .soroban/donation-id) --output-dir sdk/bindings/donation
	soroban contract bindings rust --contract-id $$(cat .soroban/campaign-id) --output-dir sdk/bindings/campaign
	soroban contract bindings rust --contract-id $$(cat .soroban/withdrawal-id) --output-dir sdk/bindings/withdrawal

bindings-typescript:
	soroban contract bindings typescript --contract-id $$(cat .soroban/donation-id) --output-dir sdk/bindings/donation-ts
	soroban contract bindings typescript --contract-id $$(cat .soroban/campaign-id) --output-dir sdk/bindings/campaign-ts
	soroban contract bindings typescript --contract-id $$(cat .soroban/withdrawal-id) --output-dir sdk/bindings/withdrawal-ts

deploy-testnet:
	./scripts/deploy.sh testnet

deploy-mainnet:
	./scripts/deploy.sh mainnet

.PHONY: build test bindings bindings-rust bindings-typescript deploy-testnet deploy-mainnet
