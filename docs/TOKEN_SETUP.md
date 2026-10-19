# AID Token Setup

## Overview

StellarAid issues a custom `AID` asset on the Stellar network using an issuing account and a distribution account.

## Setup Steps

1. Generate issuing and distribution keypairs via `src/setup/token_setup.rs`.
2. Fund both accounts on testnet using Friendbot.
3. The distribution account creates a trustline to the issuing account for the `AID` asset.
4. The issuing account sends the fixed supply to the distribution account.

## Running on Testnet

```bash
cargo run --bin worker -- setup-token
```

## Network Details

- Asset Code: `AID`
- Network: Testnet (default), Mainnet
- Horizon: [Stellar Testnet Horizon API (`https://horizon-testnet.stellar.org`)](https://horizon-testnet.stellar.org)
