# API Reference (closes #668)

Reference for the public Soroban entry points in the workspace contracts, plus
the shared helper modules. Error codes and canonical suggestions for every
value are in [the error codes reference (error_codes.md)](./error_codes.md).

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
| `create_escrow` | `(commission_id: &Bytes, client: &Address, artist: &Address, amount: i128, config_contract: &Address)` | — |
| `release_payment` | `(commission_id: &Bytes, config_contract: &Address)` | — |
| `refund_client` | `(commission_id: &Bytes, config_contract: &Address)` | — |
| `cancel_escrow` | `(commission_id: &Bytes, config_contract: &Address, artist_amount: i128, client_refund: i128)` | — |
| `expire_escrow` | `(commission_id: &Bytes, expiry_ledger: u32)` | — |
| `open_dispute` | `(commission_id: &Bytes, initiator: &Address)` | — |
| `get_escrow` | `(commission_id: &Bytes)` | `EscrowRecord` |

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

Semantics in [the idempotency specification (IDEMPOTENCY.md)](./IDEMPOTENCY.md).

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
[the timeout policy guide (TIMEOUT_POLICY.md)](./TIMEOUT_POLICY.md).

### Admin / lifecycle / ops

`pause`, `unpause`, `is_paused`; `set_dispute_ttl_ledgers`,
`get_dispute_ttl_ledgers`; `set_alert_config`, `get_alert_config`;
`set_feature_flag`, `is_feature_enabled`; `set_canary_deployment`,
`route_to_canary`, `set_rollback_trigger`, `trigger_rollback`,
`should_rollback`, `get_rollout_state`; `health_check`,
`get_health_metrics`, `get_sla_targets`, `detect_anomaly`, `report_ok`,
`report_error`. See [the pause and emergency runbook (PAUSE_AND_EMERGENCY.md)](./PAUSE_AND_EMERGENCY.md),
[the upgrade procedures (UPGRADE.md)](./UPGRADE.md), [the SLA definitions (SLA.md)](./SLA.md).

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
`PromotionNotActive=9`. Fee math details in [the fee calculation documentation (FEES.md)](./FEES.md).

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

## worker: donations list (`GET /api/donations`, closes #982)

The worker's donation feed is **append-only** — rows are inserted and their
status advances `Pending → Submitted → Confirming → Confirmed`/`Failed`, never
reordered or deleted — so the list is paginated by an **opaque cursor**, not by
`offset`. Offset pagination drifts on an append-only table: a donation confirmed
between a client's page 1 and page 2 shifts every later row, so the client sees
one record twice and misses another.

| Query parameter | Type | Meaning |
|---|---|---|
| `cursor` | string, optional | Opaque value from the previous response's `next_cursor`. Omit for the first page. |
| `limit` | integer, optional | Requested page size. `0` serves the default (`DEFAULT_PAGE_SIZE = 50`); anything above `MAX_PAGE_SIZE = 100` is capped. |
| `status` | string, optional | Filter to one lifecycle status (`Pending`, `Submitted`, `Confirming`, `Confirmed`, `Failed`, `Refunded`). |
| `campaign_id` | string, optional | Filter to one campaign. |

**Response.**

```json
{
  "donations": [ { "id": 1, "tx_hash": "...", "campaign_id": "...",
                   "donor_address": "...", "amount": 100,
                   "status": "Confirmed", "created_at": "2026-01-01T00:00:00Z" } ],
  "page": {
    "total": 137,
    "limit": 25,
    "count": 25,
    "has_more": true,
    "next_cursor": "323032362d...3a31"
  }
}
```

**Contract.**

- The cursor is opaque: pass back exactly the `next_cursor` value; do not
  construct one. A malformed cursor is rejected rather than treated as "start
  over".
- `next_cursor` is `null` and `has_more` is `false` on the last page.
- **Cursors are stable under concurrent writes.** A donation inserted between
  two requests has a later `(created_at, id)` key and therefore lands strictly
  after the current page; it never shifts or repeats a row already returned.
  This is a guarantee, not an accident, and it is proven by the repository's
  `cursor_is_stable_when_new_rows_are_inserted_between_pages` test.
- **Cursors do not expire** while the repository is alive, and remain valid
  across pruning: the cursor names an ordered `(created_at, id)` boundary, so
  removing the boundary row still resumes at the first surviving row after it.
- Filters compose with the cursor: the cursor is compared against the same
  ordered key within the filtered set, so paging a filtered list is stable too.

The pagination is implemented in `worker/src/db/donations_repo.rs`
(`DonationsRepo::list_donations`, `DonationFilter`, `PageInfo`). The HTTP route
that exposes it lands with the companion list-endpoint issue; the repository
method and response shape above are final.

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
`contracts/*/src/errors.rs` and documented in [the error codes reference (error_codes.md)](./error_codes.md).
Every contract exposes `get_suggestion(code)` and a unit test asserting each
code's Display + suggestion.