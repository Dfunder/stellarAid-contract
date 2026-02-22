.PHONY: help build test fmt lint clean wasm check-deps install-tools audit deny

# Default target
help:
	@echo "StellarAid Development Commands"
	@echo ""
	@echo "Available targets:"
	@echo "  build        - Build the WASM contract and CLI tools"
	@echo "  wasm         - Build only the WASM contract"
	@echo "  test         - Run all tests"
	@echo "  fmt          - Format code with rustfmt"
	@echo "  lint         - Run clippy linter"
	@echo "  clean        - Clean build artifacts"
	@echo "  check-deps   - Check if required dependencies are installed"
	@echo "  install-tools- Install development dependencies"
	@echo "  audit        - Check for security vulnerabilities in dependencies"
	@echo "  deny         - Check for license and ban policies"
	@echo "  help         - Show this help message"

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
test:
	@echo "Running tests..."
	cargo test --workspace
	@echo "✅ All tests passed!"

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
	@echo "✅ Development dependencies installed!"

# Quick setup for new contributors
setup: install-tools build
	@echo ""
	@echo "🎉 StellarAid development environment setup complete!"
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
