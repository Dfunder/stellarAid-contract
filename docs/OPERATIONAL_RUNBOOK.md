# Operational Runbook — Worker Service

This document describes how to operate, monitor, and troubleshoot the
StellarAid worker service.

## Service Overview

The worker is a long-running HTTP service that:

- Processes donation verification via Horizon
- Dispatches webhook notifications
- Exposes health and readiness endpoints

## Starting the Worker

```bash
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org \
HORIZON_URL=https://horizon-testnet.stellar.org \
SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 2015" \
STELLAR_PLATFORM_SECRET=<secret> \
cargo run --bin worker
```

## Health Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | JSON with uptime, donation count, error count |
| `/ready` | GET | 200 OK when ready to serve traffic |

Every Soroban contract also exposes on-chain `health_check`, `get_sla_targets`,
and `detect_anomaly`. See [SLA.md](./SLA.md) and [DEPLOY.md](./DEPLOY.md).

## Logging

Set `LOG_LEVEL` to control verbosity:

```
LOG_LEVEL=debug   # Detailed operational logs
LOG_LEVEL=info    # Normal operations (default)
LOG_LEVEL=warn    # Warnings and errors only
```

Logs are emitted as structured JSON to stdout.

## Monitoring

Key metrics to monitor:

- **Donation verification rate**: Should match expected donation throughput
- **Webhook delivery success rate**: < 100% indicates downstream issues
- **Horizon API latency**: Elevated latency may indicate rate limiting
- **Error rate**: Spikes may indicate contract or network issues

## Troubleshooting

### Webhook delivery failures

1. Check the recipient URL is reachable
2. Verify the webhook secret matches
3. Check webhook service logs for HTTP status codes
4. If the issue is transient, webhooks are retried on the next matching event

### Donation verification stuck

1. Verify Horizon endpoint is reachable
2. Check the transaction hash exists on the ledger
3. Donation status may stay at `Submitted` if Horizon is behind

### Worker won't start

1. Verify all environment variables are set (see DEPLOYMENT_CONFIGURATION.md)
2. Check the Soroban RPC URL is correct for the target network
3. Ensure the admin secret key corresponds to the contract admin

## Incident Response

### Maintenance windows

Planned pauses and upgrades run inside the windows defined in
[docs/MAINTENANCE_WINDOWS.md](../docs/MAINTENANCE_WINDOWS.md) (Tuesday /
Thursday 02:00–04:00 UTC, plus emergency). Follow that document for pause
order, state backup, upgrade timeline, and communication templates.

### Pause contracts

If a vulnerability is discovered, pause all contracts:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source $ADMIN_SECRET \
  --network testnet \
  -- \
  pause --admin <ADMIN_ADDRESS>
```

### Upgrade a contract

1. Build the new WASM
2. Call the `upgrade` function with the new WASM hash
3. Verify initialization state of the upgraded contract
4. Update ABI bindings (see scripts/generate_abi.sh)
