.PHONY: help build test test-unit test-integration test-coverage fmt lint clean wasm check-deps install-tools audit deny coverage ci-check performance-test

# Default target
help:
	@echo "StellarAid Development Commands"
	@echo ""
	@echo "Available targets:"
	@echo "  build              - Build the WASM contract and CLI tools"
	@echo "  wasm               - Build only the WASM contract"
	@echo "  test               - Run all tests (unit + integration)"
	@echo "  test-unit          - Run only unit tests"
	@echo "  test-integration   - Run only integration tests"
	@echo "  test-coverage      - Run tests with coverage report"
	@echo "  performance-test   - Run performance-focused tests"
	@echo "  fmt                - Format code with rustfmt"
	@echo "  lint               - Run clippy linter"
	@echo "  clean              - Clean build artifacts"
	@echo "  check-deps         - Check if required dependencies are installed"
	@echo "  install-tools      - Install development dependencies"
	@echo "  audit              - Check for security vulnerabilities in dependencies"
	@echo "  deny               - Check for license and ban policies"
	@echo "  coverage           - Generate coverage report"
	@echo "  ci-check           - Run all CI checks locally"
	@echo "  help               - Show this help message"

# Build everything
build: wasm
	@echo "Building CLI tools..."
	cargo build -p stellaraid-tools
	@echo "✅ Build complete!"

# Build WASM contract only
wasm:
	@echo "Building WASM contract..."
	cargo build -p stellaraid-core --target wasm32-unknown-unknown
	@echo "✅ WASM contract built: target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"

# Build release WASM contract
wasm-release:
	@echo "Building release WASM contract..."
	cargo build -p stellaraid-core --target wasm32-unknown-unknown --release
	@echo "✅ Release WASM contract built: target/wasm32-unknown-unknown/release/stellaraid_core.wasm"

# Run tests
test: test-unit test-integration
	@echo "✅ All tests passed!"

# Run unit tests only
test-unit:
	@echo "Running unit tests..."
	cargo test --workspace --lib
	@echo "✅ Unit tests passed!"

# Run integration tests only
test-integration:
	@echo "Running integration tests..."
	@start_time=$$(date +%s); \
	cargo test --workspace --test integration_tests -- --nocapture; \
	end_time=$$(date +%s); \
	duration=$$((end_time - start_time)); \
	echo "Integration tests completed in $${duration} seconds"; \
	if [ $${duration} -gt 120 ]; then \
		echo "❌ Integration tests took longer than 2 minutes ($${duration}s)"; \
		exit 1; \
	fi
	@echo "✅ Integration tests passed within time limit!"

# Run tests with coverage
test-coverage:
	@echo "Running tests with coverage..."
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo "Installing cargo-tarpaulin..."; \
		cargo install cargo-tarpaulin; \
	fi
	cargo tarpaulin --workspace --out Html --output-dir coverage --exclude-files "*/tests/*" --exclude-files "*/test_snapshots/*"
	@echo "Coverage report generated in coverage/ directory"
	@echo "Open coverage/tarpaulin-report.html in your browser"

# Run performance tests
performance-test:
	@echo "Running performance tests..."
	cargo test --workspace --release -- --nocapture performance
	@echo "✅ Performance tests completed!"

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt --all
	@echo "✅ Code formatted!"

# Run linter
lint:
	@echo "Running clippy..."
	cargo clippy --workspace -- -D warnings
	@echo "✅ Linting passed!"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete!"

# Check if required dependencies are installed
check-deps:
	@echo "Checking development dependencies..."
	@echo "Rust version:"
	@rustc --version
	@echo ""
	@echo "Available targets:"
	@rustup target list --installed
	@echo ""
	@echo "Soroban CLI:"
	@if command -v soroban >/dev/null 2>&1; then \
		soroban --version; \
	else \
		echo "❌ Soroban CLI not found. Run 'make install-tools' to install."; \
	fi
	@echo ""
	@if rustup target list --installed | grep -q "wasm32-unknown-unknown"; then \
		echo "✅ wasm32-unknown-unknown target is installed"; \
	else \
		echo "❌ wasm32-unknown-unknown target not found. Run 'rustup target add wasm32-unknown-unknown'"; \
	fi

# Install development dependencies
install-tools:
	@echo "Installing development dependencies..."
	@echo "Installing Soroban CLI..."
	cargo install soroban-cli
	@echo "Adding wasm32-unknown-unknown target..."
	rustup target add wasm32-unknown-unknown
	@echo "Installing cargo-audit..."
	cargo install cargo-audit --locked
	@echo "Installing cargo-deny..."
	cargo install cargo-deny --locked
	@echo "Installing cargo-tarpaulin for coverage..."
	cargo install cargo-tarpaulin
	@echo "✅ Development dependencies installed!"

# Quick setup for new contributors
setup: install-tools build
	@echo ""
	@echo "🎉 StellarAid development environment setup complete!"

# Generate coverage report
coverage: test-coverage

# Run all CI checks locally
ci-check: fmt lint test security-audit performance-test
	@echo "✅ All CI checks passed!"

# Security audit
audit:
	@echo "Running security audit..."
	@if ! command -v cargo-audit >/dev/null 2>&1; then \
		echo "Installing cargo-audit..."; \
		cargo install cargo-audit --locked; \
	fi
	cargo audit
	@echo "✅ Security audit passed!"

# License and dependency check
deny:
	@echo "Running cargo-deny checks..."
	@if ! command -v cargo-deny >/dev/null 2>&1; then \
		echo "Installing cargo-deny..."; \
		cargo install cargo-deny --locked; \
	fi
	cargo deny check
	@echo "✅ Dependency checks passed!"
	@echo ""
	@echo "Next steps:"
	@echo "1. Run 'make test' to verify everything works"
	@echo "2. Check the README.md for development guidelines"
	@echo "3. Start developing your feature!"

# Continuous integration target
ci: audit deny fmt lint test
	@echo "✅ CI checks passed!"

# Run security audit
audit:
	@echo "Running cargo-audit..."
	cargo audit
	@echo "✅ Security audit passed!"

# Run cargo-deny checks
deny:
	@echo "Running cargo-deny..."
	cargo deny check
	@echo "✅ cargo-deny checks passed!"
