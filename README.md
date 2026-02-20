# 🌟 StellarAid  
[![CI](https://github.com/YOUR_USERNAME/stellaraid-contract/workflows/CI/badge.svg)](https://github.com/YOUR_USERNAME/stellaraid-contract/actions)

A blockchain-based crowdfunding platform built on the **Stellar Network** for transparent, borderless, and secure fundraising.

StellarAid enables project creators to raise funds in **XLM** or any Stellar-based asset (USDC, NGNT, custom tokens), while donors can contribute with full on-chain transparency.

## � Workspace Layout

This project uses a Rust Cargo workspace with the following structure:

```
stellarAid-contract/
├── Cargo.toml                 # Workspace configuration
├── crates/
│   ├── contracts/
│   │   └── core/             # Core Soroban smart contract
│   │       ├── Cargo.toml
│   │       └── src/
│   │           └── lib.rs    # Contract implementation
│   └── tools/                # CLI utilities and deployment tools
│       ├── Cargo.toml
│       └── src/
│           └── main.rs       # CLI entry point
├── .gitignore
└── README.md
```

### Crates Overview

- **`stellaraid-core`**: Main Soroban smart contract implementing the crowdfunding logic
- **`stellaraid-tools`**: CLI utilities for contract deployment, configuration, and management

## 🛠️ Development Setup 

### Quick Start (New Contributors)

1. **Clone the repository**
   ```bash
   git clone https://github.com/YOUR_USERNAME/stellaraid-contract.git
   cd stellaraid-contract
   ```

2. **Install Rust toolchain** (automatically configured by `rust-toolchain.toml`)
   ```bash
   # Install Rust if not already installed
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # The project will automatically use the correct Rust version and targets
   rustup show
   ```

3. **Install Soroban CLI**
   ```bash
   # Method 1: Install via cargo (recommended for development)
   cargo install soroban-cli
   
   # Method 2: Install via npm (alternative)
   npm install -g soroban-cli
   
   # Verify installation
   soroban --version
   ```

4. **Build the project**
   ```bash
   # Using Make (recommended)
   make build
   
   # Or using cargo directly
   cargo build -p stellaraid-core --target wasm32-unknown-unknown
   ```

### Prerequisites
- **Rust stable toolchain** (automatically managed by `rust-toolchain.toml`)
- **wasm32-unknown-unknown target** (auto-installed by toolchain)
- **Soroban CLI** for contract deployment and testing

### Toolchain Configuration

This project uses `rust-toolchain.toml` to ensure consistent development environments:

```toml
[toolchain]
channel = "stable"
targets = ["wasm32-unknown-unknown"]
components = ["rustfmt", "clippy"]
```

This ensures:
- Consistent Rust version across all contributors
- Required targets are automatically installed
- Essential components (rustfmt, clippy) are included

### Development Commands

The project includes a Makefile for common development tasks:

```bash
# Build WASM contract
make build

# Run all tests
make test

# Format code
make fmt

# Run linter
make lint

# Clean build artifacts
make clean

# Show all available commands
make help
```

### Building (Manual)
```bash
# Build the core contract for WASM
cargo build -p stellaraid-core --target wasm32-unknown-unknown --release

# Build the CLI tools
cargo build -p stellaraid-tools

# Build entire workspace
cargo build --workspace
```

### Testing
```bash
# Run contract tests
cargo test -p stellaraid-core

# Run all tests
cargo test --workspace
```

### CLI Usage
```bash
# Check configuration
cargo run -p stellaraid-tools -- config check

# Deploy contract (placeholder)
cargo run -p stellaraid-tools -- deploy --network testnet
```
## 📌 Features

### 🎯 For Donors
- Discover global fundraising campaigns  
- Donate in XLM or Stellar assets  
- Wallet integration (Freighter, Albedo, Lobstr)  
- On-chain transparency: verify all transactions  

### 🎯 For Creators
- Create social impact projects  
- Accept multi-asset contributions  
- Real-time donation tracking  
- Withdraw funds directly on-chain  

### 🎯 For Admins
- Campaign approval workflow  
- User & KYC management  
- Analytics dashboard  

## 🏗️ Architecture Overview

StellarAid Blockchain Layer is built with: 
- Stellar Testnet / Mainnet support  
- Donation verification  
- On-chain withdrawal system  
- Asset‑agnostic design  
  
# 📌 How to Contribute

### 1. Fork the Repository
Click the **“Fork”** button in the top‑right of the GitHub repo and clone your fork:

```bash
git clone https://github.com/YOUR_USERNAME/stellaraid-contract.git
cd stellaraid-contract
````
### 2. Create a Branch
````bash
git checkout -b feature/add-donation-flow
````

### 3. Commit Messages
Use conventional commits:
````bash
feat: add wallet connection modal
fix: resolve donation API error
docs: update project README
refactor: clean up project creation form
````
### 4. Submitting a Pull Request (PR)
Push your branch:
```bash
git push origin feature/add-donation-flow
```
Open a Pull Request from your fork back to the main branch.

# 📜 License
MIT License — free to use, modify, and distribute.
