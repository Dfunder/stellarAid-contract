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
| Docker | 24.x+ | For local testing and containerized services | https://www.docker.com/get-started/ |
| VS Code | Latest | Recommended IDE with Rust/Soroban extensions | https://code.visualstudio.com/ |

### System Requirements
- **Operating System**: Windows 10+, macOS 13+, or Linux (Ubuntu 20.04+)
- **RAM**: Minimum 16GB (32GB recommended for full toolchain)
- **Storage**: 50GB free space (for Rust toolchains, Docker images, and dependencies)
- **Internet Connection**: Broadband connection for downloading dependencies and accessing Stellar testnet

## Installation Steps

### 1. Clone the Repository
```bash
git clone https://github.com/akargi/stellarAid-contract.git
cd stellarAid-contract
```

### 2. Install Rust and Targets
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown

# Install required components
rustup component add clippy rustfmt
```

### 3. Install Soroban CLI
```bash
# Install Soroban CLI using cargo
cargo install soroban-cli

# Verify installation
soroban --version
```

### 4. Install Node.js Dependencies
```bash
# Install TypeScript dependencies
npm install

# Install TypeScript SDK globally (optional)
npm install -g @stellar/stellar-sdk
```

### 5. Configure Docker Services
```bash
# Start local development containers
docker-compose up -d local-soroban-rpc horizon-postgres

# Verify all services are running
docker-compose ps
```

## Configuration Steps

### 1. Environment Setup
Create a local environment file from the template:
```bash
cp .env.example .env.local
```

Edit `.env.local` with your configuration:
```env
# Local development settings
SOROBAN_RPC_URL=http://localhost:8000/soroban/rpc
HORIZON_URL=http://localhost:8000/horizon
NETWORK_PASSPHRASE="Local SDF Network ; September 2015"

# Local accounts
ADMIN_SECRET_KEY=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
OPERATOR_SECRET_KEY=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

# Development settings
LOG_LEVEL=debug
IPFS_GATEWAY_URL=http://localhost:8080/ipfs/
```

### 2. Generate Local Stellar Accounts
```bash
# Generate a new test account
soroban keys generate local-admin

# Fund the account on local network
soroban keys fund local-admin --network local

# Export the secret key to use in .env.local
soroban keys show localadmin --secret-key
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

# Run specific contract tests
cargo test -p dispute-arbiter
cargo test -p commission-agreement
cargo test -p campaign-factory
```

### 2. Run Integration Tests
```bash
# Start local network first
docker-compose up -d

# Run end-to-end deployment tests
./scripts/test-local-deployment.sh

# Run TypeScript SDK tests
npm test
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
```bash
# Deploy all contracts to local network
./scripts/deploy.sh --network local \
  --env-file .env.local \
  --deploy-all-contracts

# Verify deployment
./scripts/verify-deployment.sh --network local
```

## Development Workflow

### 1. Create Feature Branch
```bash
git checkout -b feature/your-feature-name
```

### 2. Write and Test Code
- Follow Rust best practices and project conventions
- Write unit tests for all new functionality
- Ensure all existing tests pass
- Run linting and formatting checks

### 3. Submit Pull Request
- Push your branch and create a PR
- Ensure CI pipeline passes all checks
- Request code review from maintainers

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
**Symptom**: `Failed to connect to Soroban RPC at localhost:8000`
**Solution**:
```bash
# Check if Docker containers are running
docker-compose ps

# Restart Soroban RPC container
docker-compose restart local-soroban-rpc

# Verify logs
docker-compose logs local-soroban-rpc
```

#### 3. Account Funding Issues
**Symptom**: `Insufficient funds for transaction`
**Solution**:
```bash
# Re-fund your local account
soroban keys fund local-admin --network local

# Check account balance
soroban contract balance <YOUR_ACCOUNT_ADDRESS> --network local
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

#### 5. Docker Network Conflicts
**Symptom**: `Bind for 0.0.0.0:8000 failed: port is already allocated`
**Solution**:
```bash
# Find process using port 8000
netstat -ano | findstr :8000

# Stop conflicting process or change port in docker-compose.yml
```

### Getting Help

If you encounter issues not listed here:
1. Check the GitHub Issues page for known problems
2. Join the Stellar Developer Discord for community support
3. Contact the StellarAid maintainers at maintainers@stellar-aid.org
4. Create a new issue with detailed logs and reproduction steps

## Additional Resources

- [Stellar Developer Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://developers.stellar.org/docs/build/smart-contracts/overview)
- [StellarAid Architecture Guide](architecture.md)
- [Deployment Configuration Guide](DEPLOYMENT_CONFIGURATION.md)