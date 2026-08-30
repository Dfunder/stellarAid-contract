# API Reference (closes #668)

Reference for the public Soroban entry points in the workspace contracts, plus
the shared helper modules. Error codes and canonical suggestions for every
value are in [docs/error_codes.md](./error_codes.md).

Conventions:

- Every function below is a generated client method: values by reference for
  `Address`/`Bytes`/`Vec`, by value for scalar/`Option` scalars. `begin_call`
  etc. return a `Result`; invoke the `try_*` client variant to recover the raw
  contract error.
- Admin-only functions `require_auth` on the stored admin address
  (`PauseKey::Admin`); use `AcceptAdmin`/`transfer_admin` where offered.

## contracts/escrow

### Lifecycle

| Function | Signature (client) | Returns |
|---|---|---|
| `initialize` | `(admin: &Address)` | — sets admin, defaults |
| `create_escrow` | `(admin: &Address, token: &Address, amount: i128, recipient: &Address, ... )` | — |
| `release_payment` | `(admin: &Address, escrow_id: &Bytes)` | — |
| `refund_client` | `(admin: &Address, escrow_id: &Bytes)` | — |
| `cancel_escrow` | `(admin: &Address, escrow_id: &Bytes)` | — |
| `expire_escrow` | `(admin: &Address, escrow_id: &Bytes)` | — |
| `open_dispute` | `(admin: &Address, escrow_id: &Bytes)` | — |
| `get_escrow` | `(escrow_id: &Bytes)` | `Option<EscrowRecord>` |

`EscrowError` codes used here: `InvalidEscrow`, `NotActive`,
`OnlyRecipient`, `NoAuth`, `DisputeOpen`, `AlreadyActive`, `Expired`,
`CallTimeout=15`, `TimeoutRetriesExhausted=16`, `TimeoutNotConfigured=17`.

### Idempotency (closes #664)

| Function | Returns | Notes |
|---|---|---|
| `set_idempotency_ttl(admin, ttl_ledgers)` | `Result<(), EscrowError>` | admin-only |
| `get_idempotency_ttl()` | `u32` | |
| `claim_idempotency(admin, caller, key)` | `shared::idempotency::IdempotencyVerdict` | caches `(caller,key)` |
| `is_claimed_idempotency(admin, caller, key)` | `bool` | |
| `get_idempotency_record(admin, caller, key)` | `Option<IdempotencyRecord>` | |
| `extend_idempotency_ttl(admin, caller, key, ledgers)` | `Result<(), EscrowError>` | |

Semantics in [docs/IDEMPOTENCY.md](./IDEMPOTENCY.md).

### Timeouts (closes #665)

| Function | Returns |
|---|---|
| `set_call_timeout_policy(admin, policy)` | `Result<(), EscrowError>` (admin) |
| `get_call_timeout_policy()` | `TimeoutPolicy` |
| `begin_call(admin, op)` | `CallAttempt` |
| `call_expired(op)` | `bool` |
| `check_call_deadline(op)` | `Result<(), EscrowError>` |
| `record_call_retry(admin, op)` | `Result<CallAttempt, EscrowError>` |
| `call_retry_after(op)` | `u32` |
| `get_call_attempt(op)` | `Option<CallAttempt>` |

`TimeoutPolicy { timeout_ledgers, max_attempts }`, `CallAttempt { started_ledger,
deadline_ledger, attempts, retry_after_ledger }`. Full semantics in
[docs/TIMEOUT_POLICY.md](./TIMEOUT_POLICY.md).

### Admin / lifecycle / ops

`pause`, `unpause`, `is_paused`; `set_dispute_ttl_ledgers`,
`get_dispute_ttl_ledgers`; `set_alert_config`, `get_alert_config`;
`set_feature_flag`, `is_feature_enabled`; `set_canary_deployment`,
`route_to_canary`, `set_rollback_trigger`, `trigger_rollback`,
`should_rollback`, `get_rollout_state`; `health_check`,
`get_health_metrics`, `get_sla_targets`, `detect_anomaly`, `report_ok`,
`report_error`. See [docs/PAUSE_AND_EMERGENCY.md](./PAUSE_AND_EMERGENCY.md),
[docs/UPGRADE.md](./UPGRADE.md), [docs/SLA.md](./SLA.md).

## contracts/platform_config

### Fee configuration (closes #690)

| Function | Returns |
|---|---|
| `upsert_fee_tier(admin, min_volume, fee_bps)` | `Result<(), Error>` (admin) |
| `remove_fee_tier(admin, min_volume)` | `Result<(), Error>` (admin) |
| `get_fee_tiers()` | `Vec<FeeTier>` |
| `set_promotion(admin, start_ledger, end_ledger, fee_bps)` | `Result<(), Error>` (admin); `end_ledger==0` clears |
| `clear_promotion(admin)` | — |
| `set_referral_config(admin, bps)` | `Result<(), Error>` (admin) |
| `get_referral_config()` | `Option<ReferralConfig>` |
| `record_volume(admin, amount)` | accumulates caller volume |
| `get_volume(caller)` | `i128` |
| `resolve_effective_fee_bps(volume)` | `u32` — tier/promo/clamp resolution |
| `compute_fees(amount, volume)` | `FeeBreakdown` |
| `is_promotion_active()` | `bool` |

`Error` codes: `InvalidTier=6`, `InvalidPromotion=7`, `InvalidReferralBps=8`,
`PromotionNotActive=9`. Fee math details in [docs/FEES.md](./FEES.md).

### Token metadata / admin

`set_token_metadata`, `get_token_metadata`, `set_fee_bps`, `set_platform_wallet`,
`get_config`, `initialize`, `transfer_admin`, `accept_admin`.

### Lifecycle / ops

Same `pause`/`unpause`, alerting, feature flags, canary/rollout, health, SLA,
and anomaly surface as `escrow` (see above).

## contracts/shared

### `idempotency` (closes #664)

- `DEFAULT_TTL` constant; `default_ttl()`.
- `set_idempotency_ttl(env, caller, ttl)`, `get_idempotency_ttl(env)` — TTL
  lives in instance storage.
- `claim(env, caller, key) -> IdempotencyVerdict` — returns the cached record
  or records a fresh claim (persistent, TTL-extended).
- `resolve(env, caller, key) -> bool`, `get_record`, `is_claimed`,
  `extend_ttl(env, caller, key, ledgers)`.

`IdempotencyVerdict { accepted, retry_count }`, `IdempotencyRecord {
first_caller, first_ledger, last_ledger, retry_count }`.

### `timeout` (closes #665)

- `DEFAULT_TIMEOUT_LEDGERS=60`, `DEFAULT_MAX_ATTEMPTS=5`,
  `BACKOFF_BASE_LEDGERS=2`; `default_policy()`.
- `set_timeout_policy`, `get_timeout_policy` — instance storage.
- `begin_call(env, caller, key) -> CallAttempt`.
- `is_expired`, `require_not_expired`, `record_attempt` (back-off
  `retry_after_ledgers = BACKOFF_BASE * 2^attempts`), `retry_after`,
  `get_attempt`.
- `TimeoutError { Expired=1, TooManyAttempts=2, Unconfigured=3 }` with
  `code()`; `with_fallback(f, default)`.

## Usage example (idempotent + time-limited release)

```rust
use shared::idempotency::{claim, IdempotencyVerdict};

let verdict: IdempotencyVerdict = claim(&env, &caller, &key);
match verdict.accepted {
    true => { /* first time: safe to run the release */ }
    false => { /* retry: caller already saw a result, return it */ }
}
```

Run the crate doc-tests for interactive copies: `cargo test --doc -p shared`.

## Error codes

Numeric code → cause → suggestion handle mapping is defined once in
`contracts/*/src/errors.rs` and documented in [docs/error_codes.md](./error_codes.md).
Every contract exposes `get_suggestion(code)` and a unit test asserting each
code's Display + suggestion.