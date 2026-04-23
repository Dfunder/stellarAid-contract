.PHONY: build build-wasm build-tools test fmt lint clean help

# Default target
build: build-wasm build-tools
	@echo "✅ Build complete"

# Build WASM contract
build-wasm:
	@echo "🔨 Building Soroban contract..."
	cargo build -p stellaraid-core --target wasm32-unknown-unknown --release
	@echo "✅ WASM contract built successfully"

# Build CLI tools
build-tools:
	@echo "🔨 Building CLI tools..."
	cargo build -p stellaraid-tools
	@echo "✅ CLI tools built successfully"

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test --workspace
	@echo "✅ Tests passed"

# Format code
fmt:
	@echo "🎨 Formatting code..."
	cargo fmt --all
	@echo "✅ Code formatted"

# Run linter
lint:
	@echo "🔍 Running linter..."
	cargo clippy --workspace -- -D warnings
	@echo "✅ Linting passed"

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete"

# Show help
help:
	@echo "Available commands:"
	@echo "  make build       - Build WASM contract and CLI tools"
	@echo "  make build-wasm  - Build Soroban WASM contract only"
	@echo "  make build-tools - Build CLI tools only"
	@echo "  make test        - Run all tests"
	@echo "  make fmt         - Format code"
	@echo "  make lint        - Run linter"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make help        - Show this help message"
