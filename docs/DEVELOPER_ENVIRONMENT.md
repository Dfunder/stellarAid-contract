# Developer Environment Setup Guide

This guide provides step-by-step instructions for setting up a local development environment for the StellarAid platform.

## Prerequisites

Before you begin, ensure you have the following software installed on your system:

### Required Software
| Software | Version | Description | Installation Link |
|----------|---------|-------------|-------------------|
| Rust | 1.75+ | Primary language for smart contract development | https://www.rust-lang.org/tools/install |
| Soroban CLI | Latest | Stellar's smart contract development toolkit | https://developers.stellar.org/docs/build/sdks-and-tools/soroban-cli |
| Node.js | 18.x+ | For TypeScript SDK and frontend development | https://nodejs.org/ |
| Git | 2.40+ | Version control | https://git-scm.com/downloads |
| Docker | 24.x+ | Optional: containerized build and test (`make docker-build`) | https://www.docker.com/get-started/ |
| VS Code | Latest | Recommended IDE with Rust/Soroban extensions | https://code.visualstudio.com/ |

### System Requirements
- **Operating System**: Windows 10+, macOS 13+, or Linux (Ubuntu 20.04+)
- **RAM**: Minimum 16GB (32GB recommended for full toolchain)
- **Storage**: 50GB free space (for Rust toolchains, Docker images, and dependencies)
- **Internet Connection**: Broadband connection for downloading dependencies and accessing Stellar testnet

## Installation Steps

### 1. Clone the Repository
```bash
git clone https://github.com/Dfunder/stellarAid-contract.git
cd stellarAid-contract
```

If you plan to open a pull request, fork `Dfunder/stellarAid-contract` on
GitHub first and clone your fork instead.

### 2. Install Rust and Targets
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown

# Install required components
rustup component add clippy rustfmt
```

The pinned toolchain lives in `rust-toolchain.toml`. `rustup` selects it
automatically inside the repository.

### 3. Install Soroban CLI
```bash
# Same command as CONTRIBUTING.md
cargo install --locked soroban-cli --features opt

# Verify installation
soroban --version
```

### 4. Install Node.js Dependencies
The repository root has no `package.json`. The TypeScript packages live under
`sdk/`:
```bash
cd sdk/bindings
npm ci
cd ../..
```

### 5. Docker (optional)
The repository has no `docker-compose.yml` and does not run a local Soroban
network. Development and deployment target the public **testnet**. Docker is
only used to build and test the contracts in a container, via the `Dockerfile`:
```bash
make docker-build   # docker build -t stellaraid-contracts .
make docker-test    # builds the Dockerfile's "test" stage
```

## Configuration Steps

### 1. Environment Setup
Create your environment file from the template, as in
[CONTRIBUTING.md](../CONTRIBUTING.md):
```bash
cp .env.example .env
```

`.env` is read by the worker (`worker/src/main.rs`). The template defaults
point to testnet:
```env
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
HORIZON_URL=https://horizon-testnet.stellar.org
SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"

# Contract ID (C...) of the deployed donation contract. Required for donations.
DONATION_CONTRACT_ID=

BIND_ADDRESS=0.0.0.0:3000
```

Never put secret keys in `.env` or commit them. Deployment secrets are read
from environment variables (see [Local Deployment Test](#4-local-deployment-test)).

### 2. Generate Testnet Stellar Accounts
```bash
# Generate a new test identity
soroban keys generate dev-admin

# Fund it from the testnet friendbot
soroban keys fund dev-admin --network testnet

# Show the secret key (for STELLAR_PLATFORM_SECRET; keep it out of git)
soroban keys show dev-admin
```

### 3. Configure IDE Extensions
Install these VS Code extensions for optimal development:
- **rust-analyzer** - Rust language support
- **Soroban** - Stellar smart contract tools
- **CodeLLDB** - For debugging Rust code
- **Even Better TOML** - For TOML configuration files
- **GitLens** - Enhanced Git integration

## Testing Setup

### 1. Run Smart Contract Tests
```bash
# Run all Rust contract tests
cargo test --all

# Run tests with output
cargo test -- --nocapture

# Run specific contract tests (use the package names from each Cargo.toml)
cargo test -p dispute_arbiter
cargo test -p commission_agreement
cargo test -p escrow
```

### 2. Run SDK Tests and Workspace Gates
```bash
# TypeScript SDK: typecheck + lint + test + build
(cd sdk/bindings && npm run check)

# The same gates CI runs (see CONTRIBUTING.md)
make fmt lint test
./scripts/security_lint.sh
```

### 3. Code Quality Checks
```bash
# Run clippy for linting
cargo clippy --all-targets --all-features -- -D warnings

# Format all code
cargo fmt --all

# Check formatting before commit
cargo fmt --all --check
```

### 4. Local Deployment Test
Deployments target testnet. Run the preflight gate first, then do a dry run
before deploying for real:
```bash
export STELLAR_PLATFORM_SECRET=...   # from `soroban keys show dev-admin`; never commit it

# Full preflight gate (fmt, clippy, tests, WASM build, env, network probe)
make preflight

# Dry run, then deploy to testnet
./scripts/deploy.sh testnet --dry-run
./scripts/deploy.sh testnet

# Verify the deployment (reads config/testnet_contracts.json)
./scripts/verify_deployment.sh testnet
```

See [DEPLOYMENT.md](DEPLOYMENT.md) for the full pipeline (`make deploy-all`,
`make deploy-verify`, mainnet gates).

## Development Workflow

### 1. Create Feature Branch
```bash
git checkout -b feature/your-feature-name
```

### 2. Write and Test Code
- Follow Rust best practices and project conventions
- Write unit tests for all new functionality
- Ensure all existing tests pass
- Install the pre-commit hooks (`pre-commit install`) as described in
  [CONTRIBUTING.md](../CONTRIBUTING.md)

### 3. Submit Pull Request
- Use conventional commit messages (`feat:`, `fix:`, `docs:`, ...)
- One issue per PR
- Push your branch to your fork and open a PR against `Dfunder/stellarAid-contract`
- Ensure CI pipeline passes all checks

## Troubleshooting

### Common Issues and Solutions

#### 1. Rust Build Failures
**Symptom**: `error: couldn't read file ... No such file or directory`
**Solution**:
```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cargo clean
cargo build
```

#### 2. Soroban RPC Connection Errors
**Symptom**: `Failed to connect to Soroban RPC`
**Solution**:
```bash
# Check that SOROBAN_RPC_URL in .env points to a reachable endpoint
grep SOROBAN_RPC_URL .env

# Probe the environment and the target network
make preflight-env
```

#### 3. Account Funding Issues
**Symptom**: `Insufficient funds for transaction`
**Solution**:
```bash
# Re-fund your testnet identity from friendbot
soroban keys fund dev-admin --network testnet

# Check the account balance on Horizon
curl "https://horizon-testnet.stellar.org/accounts/$(soroban keys address dev-admin)"
```

#### 4. WASM Compilation Errors
**Symptom**: `unable to compile to wasm32-unknown-unknown`
**Solution**:
```bash
# Verify wasm target is installed
rustup target list --installed

# Add if missing
rustup target add wasm32-unknown-unknown
```

#### 5. Worker Port Conflicts
**Symptom**: `Address already in use` when starting the worker
**Solution**:
```bash
# Find the process using port 3000 (Windows)
netstat -ano | findstr :3000
# macOS / Linux
lsof -i :3000

# Stop the conflicting process or change BIND_ADDRESS in .env
```

### Getting Help

If you encounter issues not listed here:
1. Check the [GitHub Issues](https://github.com/Dfunder/stellarAid-contract/issues) page for known problems
2. Join the Stellar Developer Discord for community support
3. Open a new issue with detailed logs and reproduction steps

## Additional Resources

- [Stellar Developer Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://developers.stellar.org/docs/build/smart-contracts/overview)
- [StellarAid Architecture Guide](architecture.md)
- [Deployment Configuration Guide](DEPLOYMENT_CONFIGURATION.md)