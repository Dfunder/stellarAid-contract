# Deployment Guide

## Prerequisites

- Rust + `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- Soroban CLI: `cargo install --locked soroban-cli`
- Copy `.env.example` to `.env` and fill in your values

## Network Setup

```bash
soroban network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"

soroban network add mainnet \
  --rpc-url https://soroban.stellar.org \
  --network-passphrase "Public Global Stellar Network ; September 2015"
```

## Deploy

```bash
./scripts/deploy.sh testnet
./scripts/deploy.sh mainnet
```

## Invoke Example

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- hello
```

## Gradual rollout (closes #684)

New WASM must never take 100% of production traffic on the first deploy.
Every contract exposes the same rollout surface (see `shared::rollout`):

| Entry point | Purpose |
|-------------|---------|
| `set_canary_deployment(admin, canary, stable, canary_bps)` | Register canary + stable IDs and the canary traffic share (0–10000 bps) |
| `route_to_canary(caller)` | Sticky split: SHA-256(caller) mod 10000 < `canary_bps` |
| `set_feature_flag(admin, flag, enabled)` | Named feature flags for incomplete behavior |
| `is_feature_enabled(flag)` | Read a flag; always `false` after rollback |
| `set_rollback_trigger(admin, error_bps)` | Error-rate that arms automatic rollback (default 500 = 5%) |
| `should_rollback()` | `true` when the trigger is met or phase is `RolledBack` |
| `trigger_rollback(admin)` | Zero canary share, disable flags, pause the contract |
| `get_rollout_state()` | Phase, share, contract IDs, trigger, flag count |
| `health_check()` | Health report; auto-rolls back the canary when the trigger fires |

Gateway / SDK routing: call `route_to_canary` on the **stable** contract. If it
returns `true`, submit the user transaction to the canary contract ID; otherwise
keep using stable.

### Feature flags

Use flags for behavior that is compiled into both versions but must stay off
until the canary is healthy:

```bash
soroban contract invoke --id $STABLE --network testnet --source $ADMIN -- \
  set_feature_flag --admin $ADMIN_ADDR --flag new_fee --enabled true

soroban contract invoke --id $STABLE --network testnet -- \
  is_feature_enabled --flag new_fee
```

Keep flag names ≤ 9 characters if you use `symbol_short!` in tests; longer
names are fine via `Symbol::new`.

### Canary procedure

1. Deploy the new WASM to a **new** contract ID (`CANARY`). Keep `STABLE`.
2. Initialize the canary with the same admin and config as stable.
3. Pause is **not** required on stable; the canary can start paused until
   smoke tests pass, then `unpause`.
4. Register the pair and start at 1% (100 bps):

```bash
soroban contract invoke --id $STABLE --network testnet --source $ADMIN -- \
  set_canary_deployment \
    --admin $ADMIN_ADDR \
    --canary $CANARY \
    --stable $STABLE \
    --canary_bps 100
```

5. Point the gateway at `route_to_canary`. Watch `health_check` on **both**
   IDs for at least the monitoring window in [SLA.md](./SLA.md) (default: 24h
   of healthy samples before raising the share).
6. Raise `canary_bps` in steps: 100 → 500 → 1000 → 2500 → 5000 → 10000.
   Never skip to 10000 unless testnet soak already passed at 5000.
7. At 10000 bps the phase becomes `Full`. Update client config to the canary
   ID as the new stable, then retire the old ID after 30 days
   (see [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md)).

### Rollback triggers

Automatic: `health_check` calls `maybe_auto_rollback` when status is not
Healthy. If sampled `error_bps >= rollback_error_bps` (default 500), the
contract sets phase `RolledBack`, `canary_bps = 0`, and disables every
feature flag. Gateways then receive `route_to_canary == false`.

Manual (page / confirmed incident):

```bash
soroban contract invoke --id $STABLE --network testnet --source $ADMIN -- \
  trigger_rollback --admin $ADMIN_ADDR
```

That also pauses the contract via `shared::pause`. For escrow, also call
the native `pause` entry point — it uses a separate pause key.

Arm a stricter trigger before a risky canary:

```bash
soroban contract invoke --id $STABLE --network testnet --source $ADMIN -- \
  set_rollback_trigger --admin $ADMIN_ADDR --error_bps 200
```

### Rollback criteria (same as upgrade runbook)

Trigger rollback if any of the following hold during the soak window:

- Token transfers fail or produce incorrect amounts
- Event payloads are malformed or missing
- Unexpected panics or error-rate ≥ trigger
- `health_check` reports `Unhealthy` on the canary
- Storage contains corrupted or missing data

After rollback: keep traffic on stable, file an incident, fix the WASM, and
start again at 1% on a **new** canary ID. Do not reuse a rolled-back canary
instance.

## Contract health endpoints (closes #678)

```bash
soroban contract invoke --id $CONTRACT_ID --network testnet -- health_check
soroban contract invoke --id $CONTRACT_ID --network testnet -- get_health_metrics
soroban contract invoke --id $CONTRACT_ID --network testnet -- get_sla_targets
soroban contract invoke --id $CONTRACT_ID --network testnet -- detect_anomaly
```

SLA definitions, alert configuration, and monitor duties: [SLA.md](./SLA.md).
