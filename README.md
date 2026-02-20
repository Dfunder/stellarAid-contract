# 🌟 StellarAid  
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

### Prerequisites
- Rust 1.70+ with `wasm32-unknown-unknown` target
- Soroban CLI tools

### Installation
```bash
# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Soroban CLI (if not already installed)
cargo install soroban-cli
```

### Building
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
