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

## Common Errors and Debugging Strategies

### Contract invocation fails with a `ContractError` code

1. Match the numeric code against `docs/error_codes.md`. Every workspace
   contract has a `get_suggestion(code)` helper returning a canonical
   remediation handle (e.g. `CAL_TO`, `TOO_RTY`, `NO_TO_P`, `FEE_TIER`,
   `PROMO_ACT`).
2. Reproduce with a sandbox test: the code path that threw is in the contract
   source, and every error has a matching unit test in the crate's
   `*_tests.rs`/`tests.rs`.
3. If the error is transient (a timeout or a retry budget), re-submit with the
   documented backoff rather than changing contract logic.

### `Authorization` denied / `require_auth` fails

- The submitting account is not the stored admin. Read the stored admin via
  the contract's `get_admin`/`admin` entry point and compare.
- The account was never authenticated for the operation (off-chain key vs
  on-chain address mismatch, or the caller didn't sign the invocation).
- Check `PauseKey` gating: a paused contract rejects admin mutations until
  `unpause`.

### Data not visible (record reads return `None`)

- Persistent records are TTL-managed and can expire in ~`<TTL>` ledgers without
  extension. Re-run the write path and ensure `extend_ttl` fires.
- You may be querying a different contract instance (dev vs testnet vs prod
  address). Confirm the `CONTRACT_ID` used to read matches the writer.

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

## Solution Examples

| Symptom | Root cause | Solution |
|---|---|---|
| `NO_TO_P` (timeout not configured) | no `TimeoutPolicy` set | `set_call_timeout_policy` with a ledger count + max attempts |
| Op fails after the deadline | caller waited past `deadline_ledger` | re-run `begin_call`; treat the prior attempt as expired, not failed |
| `TOO_RTY` (retries exhausted) | caller exceeded `max_attempts` while backing off | wait for `retry_after` ledgers, then `begin_call` a fresh cycle |
| Duplicate idempotency claim on retry | same `(caller, key)` re-submission | claim returns the cached verdict instead of re-executing |
| Fee outside configured band | malformed tier/promotion/referral write | fix config; effective rate is clamped to `MIN_FEE_BPS`..`MAX_FEE_BPS` |

## Recovery Procedures

### Recover from a failed upgrade

1. Pause affected contracts within the maintenance window.
2. Restore the previous WASM hash via `upgrade` (kept in CI artifacts / release
   notes, see `docs/UPGRADE_AND_ROLLBACK.md`).
3. State written by the new version must be rolled back only if schema-breaking
   happened; schema-compatible upgrades preserve all records (see
   `docs/UPGRADE_TESTING.md` run 1–2).
4. Re-run the regression suite against the restored contract before
   unpausing.

### Recover a contract stuck in timeout-pending state

- Verify `is_expired`/`call_expired` against the current ledger.
- Calling parties whose attempts have expired get fresh `begin_call` starts;
  no manual data repair is needed.

### Retry and notification back-off

- Timeout module back-off: `retry_after_ledgers = base * 2^attempt`, capped by
  `max_attempts`.
- Webhooks: retried on the next matching event; monitor delivery success in
  logs as described under Monitoring.

## FAQ

- **Why do I see a panic with a `suggestion` handle but no message?** Wide
  contract errors are deliberately opaque on-chain; `get_suggestion`/the docs
  map the code to the cause. See `docs/error_codes.md`.
- **How do I rotate the admin?** Re-initialize admin-only settings with the new
  `Address`; any `require_auth`-gated function (e.g. `set_idempotency_ttl`,
  `upsert_fee_tier`) is the migration hook. There is no shared
  admin-overwrite path by design.
- **A record "disappeared".** Likely TTL expiry, not data loss. Persistent
  records without renewal drop after their configured TTL (check the crate's
  TTL constants, e.g. `VOLUME_TTL_LEDGERS`, and `extend_ttl` usage). Re-issue
  the triggering transaction.
- **Can upgrades be reversed after confirmation?** Yes, by deploying the prior
  WASM hash — the previous version never assumed network state, only storage.
- **What if the worker and contract disagree on fee rate?** Both must read
  `resolve_effective_fee_bps`/`compute_fees` at the same ledger. Run both from
  the same source of truth (`platform_config`) rather than hard-coding.
