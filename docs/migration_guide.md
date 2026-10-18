# Contract Upgrade Migration Guide

## Overview

This guide describes the steps required to migrate StellarAid contracts to the latest version.

## Prerequisites

- Stellar CLI >= 0.9.0
- Soroban SDK >= 0.9.0
- Admin keypair with upgrade authority

## Migration Steps

### Step 1: Deploy New Contract

Deploy the updated contract to the network:

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stellar_aid.wasm \
  --source admin \
  --network mainnet
```

### Step 2: Upgrade Existing Contract

Run the upgrade instruction on the existing contract:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network mainnet \
  -- upgrade \
  --new_wasm_hash <NEW_WASM_HASH>
```

### Step 3: Migrate Storage

If storage schema changed, run the migration helper:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  -- migrate_storage
```

### Step 4: Verify Upgrade

Confirm the contract version is updated:

```bash
stellar contract invoke --id <CONTRACT_ID> -- version
```

## Rollback

If issues occur, redeploy the previous WASM and invoke upgrade with the old hash.

## Changelog

- Added multi-chain asset support (#581)
- Added creator cooperative DAO (#580)
- Added decentralized dispute resolution (#578)
