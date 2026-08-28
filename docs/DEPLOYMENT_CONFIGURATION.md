# Deployment Configuration

This document describes environment variables, secret management, network configurations, and deployment procedures for deploying the StellarAid platform.

## Environment Variables

All deployments require the following environment variables:

| Variable | Required | Description |
|----------|----------|-------------|
| `SOROBAN_RPC_URL` | Yes | Soroban RPC endpoint for the target network |
| `HORIZON_URL` | Yes | Horizon API endpoint for the target network |
| `NETWORK_PASSPHRASE` | Yes | Network passphrase for the target Stellar network |
| `CONTRACT_DIR` | No | Path to compiled `.wasm` files (default: `./contracts`) |
| `ADMIN_SECRET_KEY` | Yes | Admin Stellar secret key (seed) for deployment transactions |
| `OPERATOR_SECRET_KEY` | Yes | Operator secret key for day-to-day contract operations |
| `EMERGENCY_MULTISIG_KEYS` | Yes (mainnet only) | List of public keys for emergency multisig signers |
| `LOG_LEVEL` | No | Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`) |
| `MONITORING_WEBHOOK_URL` | No | Webhook URL for deployment and operational alerts |
| `IPFS_GATEWAY_URL` | No | IPFS gateway for off-chain metadata storage (default: `https://ipfs.io/ipfs/`) |

## Secrets Management

Sensitive values must never be committed to the repository.

- Store all secret keys (`ADMIN_SECRET_KEY`, `OPERATOR_SECRET_KEY`) in a vault (e.g., HashiCorp Vault, AWS Secrets Manager, 1Password)
- Use environment-specific `.env` files that are git-ignored
- Rotate keys between testnet and mainnet deployments
- Use separate admin/operator accounts for production deployments
- Enable 2FA for all accounts that manage mainnet deployments

## Testnet Configuration

Use these settings for Stellar testnet deployments:

| Parameter | Value |
|-----------|-------|
| **Soroban RPC URL** | `https://soroban-testnet.stellar.org` |
| **Horizon URL** | `https://horizon-testnet.stellar.org` |
| **Network Passphrase** | `Test SDF Network ; September 2015` |
| **Friendbot URL** | `https://friendbot.stellar.org` |
| **Stellar Explorer** | `https://testnet.steexp.com` |
| **Minimum Reserve** | 1.5 XLM per account |

### Testnet Deployment Command
```bash
./scripts/deploy.sh --network testnet \
  --env-file .env.testnet \
  --deploy-all-contracts
```

## Mainnet Configuration

Use these settings for Stellar mainnet deployments:

| Parameter | Value |
|-----------|-------|
| **Soroban RPC URL** | `https://soroban-mainnet.stellar.org` (or your private RPC endpoint) |
| **Horizon URL** | `https://horizon.stellar.org` (or your private Horizon instance) |
| **Network Passphrase** | `Public Global Stellar Network ; September 2015` |
| **Stellar Explorer** | `https://steexp.com` |
| **Minimum Reserve** | 2.5 XLM per account (updated for Soroban) |

### Mainnet Prerequisites
- Pre-fund admin/operator accounts with sufficient XLM (minimum 1000 XLM recommended for deployment)
- Configure emergency multisig with at least 3/5 signers
- Set up monitoring and alerting before deployment
- Perform a full staging deployment on testnet first

### Mainnet Deployment Command
```bash
./scripts/deploy.sh --network mainnet \
  --env-file .env.mainnet \
  --require-multisig-approval \
  --deploy-all-contracts
```

## Network Configuration Script Options

The `scripts/deploy.sh` script supports all networks with these flags:

```
--dry-run          Validate configuration without deploying
--wasm <path>      Path to a specific contract WASM file
--admin <address>  Override configured admin address
--network <name>   Target network (testnet/mainnet)
--env-file <path>  Path to environment file
--deploy-all       Deploy and initialize all contracts in sequence
--verify-only      Verify existing contract deployments
```

## Initialization Sequence

Contracts must be initialized in this specific order to ensure proper dependency resolution:

1. **Deploy and initialize `dispute_arbiter`**
   ```bash
   soroban contract invoke \
     --id <DISPUTE_ARBITER_ID> \
     --rpc-url $SOROBAN_RPC_URL \
     --network-passphrase "$NETWORK_PASSPHRASE" \
     --source $ADMIN_SECRET_KEY \
     -- \
     initialize \
     --admin <ADMIN_ADDRESS> \
     --operator <OPERATOR_ADDRESS> \
     --arbitration-period 259200  # 30 days in seconds
   ```

2. **Deploy and initialize `commission_agreement`**
   ```bash
   soroban contract invoke \
     --id <COMMISSION_AGREEMENT_ID> \
     --rpc-url $SOROBAN_RPC_URL \
     --network-passphrase "$NETWORK_PASSPHRASE" \
     --source $ADMIN_SECRET_KEY \
     -- \
     initialize \
     --admin <ADMIN_ADDRESS> \
     --dispute-arbiter <DISPUTE_ARBITER_ID> \
     --platform-fee-bps 250  # 2.5% platform fee
   ```

3. **Deploy and initialize `campaign_factory`**
   ```bash
   soroban contract invoke \
     --id <CAMPAIGN_FACTORY_ID> \
     --rpc-url $SOROBAN_RPC_URL \
     --network-passphrase "$NETWORK_PASSPHRASE" \
     --source $ADMIN_SECRET_KEY \
     -- \
     initialize \
     --admin <ADMIN_ADDRESS> \
     --commission-agreement <COMMISSION_AGREEMENT_ID> \
     --max-campaign-duration 31536000  # 1 year in seconds
   ```

4. **Configure cross-contract permissions**
   - Grant factory address permission to create new campaigns
   - Set up emergency pause capabilities for all contracts
   - Verify all role assignments are correct

## Health Checks

After deployment, verify the worker services are operational:

- `GET /health` — JSON with uptime, donation count, error count, last activity
- `GET /ready` — 200 OK when the worker is ready to serve traffic
- `GET /metrics` — Prometheus metrics for monitoring systems

## CI/CD Pipeline

The `.github/workflows/ci.yml` workflow:

1. Builds all Rust contracts (wasm32 target)
2. Runs Rust test suite
3. Runs TypeScript SDK tests
4. Runs clippy and rustfmt for code quality checks
5. Generates deployment artifacts for staging/production

## Deployment Validation Checklist

### Pre-Deployment
- [ ] All contract WASM files are compiled and verified with `soroban contract inspect`
- [ ] Environment variables correctly set for target network
- [ ] All accounts have sufficient XLM balance for deployment fees
- [ ] Secret keys are stored securely in vault provider
- [ ] Emergency multisig configured (mainnet only)
- [ ] Monitoring and alerting webhooks configured
- [ ] Previous testnet deployment successful and verified
- [ ] Security review completed for mainnet deployment

### During Deployment
- [ ] Deploy contracts in correct initialization order
- [ ] Verify each transaction succeeds on-chain via explorer
- [ ] Save all deployed contract IDs to version-controlled inventory
- [ ] Validate initialize function parameters match requirements
- [ ] Cross-contract permissions properly configured
- [ ] Emergency pause functionality tested on all contracts

### Post-Deployment
- [ ] Verify all contracts are active and responsive
- [ ] Test creating a campaign through the factory
- [ ] Execute a test donation to verify payment flow
- [ ] Submit a test dispute to validate arbiter functionality
- [ ] Worker health and readiness endpoints return 200 OK
- [ ] Monitoring system receiving contract event data
- [ ] Document all contract IDs in the operations runbook
- [ ] Rotate any temporary deployment keys (mainnet)
- [ ] Perform full backup of all deployment configuration
- [ ] Notify stakeholders of successful deployment