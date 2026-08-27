# Contract SLA Metrics

On-chain health checks (closes #678) use these service-level objectives. Every
contract exposes `get_sla_targets` and `health_check` so monitors do not have
to hard-code the numbers.

## Objectives

| Metric | Target | On-chain encoding |
|--------|--------|-------------------|
| Availability | 99.90% | `availability_bps = 9990` |
| Error rate | ≤ 0.10% | `max_error_bps = 10` |
| Degraded threshold | ≥ 1% errors **or** stalled | `degraded_error_bps = 100` |
| Unhealthy threshold | ≥ 5% errors **or** paused | `unhealthy_error_bps = 500` |
| Activity stall | No samples for ~1 day | `stall_ledgers = 17280` |
| Health-check freshness | Poll at least every ~5 min | `health_check_max_ledgers = 60` |

Ledger counts assume ~5 second ledgers. Adjust if the network cadence changes.

## Status mapping

| Status | Meaning | Operator action |
|--------|---------|-----------------|
| `Healthy` | Error rate below degraded threshold, not paused, not stalled | None |
| `Degraded` | Error rate ≥ 1% or no activity within `stall_ledgers` | Investigate; freeze canary expansion |
| `Unhealthy` | Error rate ≥ 5% or contract paused | Page on-call; consider `trigger_rollback` |

`detect_anomaly` is `true` for both `Degraded` and `Unhealthy`.

## Alerting

`set_alert_config` stores thresholds and a cooldown. When `health_check` finds
an anomaly and alerting is enabled, the contract emits `hlth_alrt` with
`(status, error_bps, stalled)`, at most once per `alert_cooldown_ledgers`.

Off-chain monitors should:

1. Invoke `health_check` on every deployed contract ID on the freshness
   interval above.
2. Subscribe to `hlth_alrt` and `rollback` events on Horizon / Soroban RPC.
3. Treat a missed poll beyond `health_check_max_ledgers` as a monitor failure,
   not a contract failure.

## Sampling

`report_ok` / `report_error` (admin-auth) increment the counters that feed
`error_bps`. Wire the worker or a sidecar to report sampled invoke outcomes so
the on-chain rate tracks real traffic.

## Related

- Rollout, canary, and automatic rollback: [DEPLOY.md](./DEPLOY.md)
- Worker HTTP `/health`: [OPERATIONAL_RUNBOOK.md](./OPERATIONAL_RUNBOOK.md)
