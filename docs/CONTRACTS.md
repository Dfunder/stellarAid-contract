# Contract Specification

Reference documentation for every crate in the stellarAid workspace: what it is
for, who may call it, what it emits, and how it fails.

> - **Storage layout and TTL policy:** [STORAGE.md](./STORAGE.md).
> - **Event schemas:** [EVENTS.md](./EVENTS.md).
> - **Error code catalogue:** [error_codes.md](./error_codes.md).
> - **Bump rules and compatibility:** [VERSIONING.md](./VERSIONING.md).

## Scope

This covers the **24 workspace members** under `contracts/`. Two directories
exist but are *not* workspace members and are therefore not deployed or
documented here: `contracts/donation/` and `contracts/withdrawal/`. Earlier
revisions of this document described `campaign`, `donation` and `withdrawal` as
the system; the workspace has since grown to 24 crates and this revision
replaces that architecture sketch with per-crate reference material.

`shared` is a plain Rust library, not a contract. It declares no `#[contract]`
type and no `#[contractimpl]` block, so it contributes no entry points of its
own — its functions are generic over `&Env` and are inlined into the calling
contract's WASM, which means they act on **the caller's** storage namespaces.
It is documented last because 23 of the 24 members depend on it.

## Conventions used below

- **Authorization** states who must sign. `none (view)` means the entry point
  reads state and requires no signature. Where a contract takes an `admin`
  parameter and calls `require_auth()` on it *without* comparing it to the
  stored admin, that is called out explicitly — it is a real property of the
  code, not an oversight in this document.
- **Events** are listed per unique `publish` shape. Some payloads are a bare
  value rather than a tuple; where that matters for an indexer, it is noted.
- **Errors** list every `#[contracterror]` variant with its numeric code. Some
  variants are never constructed because authorization fails earlier at
  `require_auth()`; those are marked, because an unreachable variant is a
  common source of false confidence in a decoded error.

## Architecture

```
                    ┌──────────────┐
                    │ platform_config │  feature flags, fee bps, token ids
                    └───────┬──────┘
                            │ cross-contract call (get_fee_bps, get_usdc, …)
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼───────┐  ┌────────▼────────┐  ┌───────▼────────┐
│    escrow     │  │commission_agreement│ │dispute_arbiter│
│  custody      │  │ milestones, fees  │ │ arbitration    │
└───────┬───────┘  └────────┬────────┘  └───────┬────────┘
        │                    │                   │
        │           ┌────────▼────────┐          │
        │           │   analytics     │  performance ledger
        │           └─────────────────┘
        │
┌───────▼───────┐  ┌─────────────────┐  ┌─────────────────┐
│   campaign    │  │  revenue_sharing│  │   rate_limiter  │
│  fundraising  │  │  payouts        │  │  abuse control  │
└───────────────┘  └─────────────────┘  └─────────────────┘
```

This is a sketch of the *domains*, not a call graph. The authoritative statement
of who calls whom is the **Dependencies** row under each contract below, and the
**Notes** column of each public entry point.

## Versioning

Every contract crate uses semantic versioning and exposes `get_version()`,
`get_version_metadata()` (name, min-compatible client, storage schema) and
`is_version_compatible(major, minor, patch)`. These come from one of two places,
and the distinction matters:

- Crates depending on `shared` install them via `impl_semver_queries!()`, which
  consults `UpgradeKey::Version` in instance storage.
- Crates that do not depend on `shared` get the Cargo.toml-only twin from
  `contracts/semver_types.rs` via `include!`, which reads **no storage** and
  therefore cannot reflect an on-chain upgrade.

`rate_limiter` implements neither, so those three entry points are absent from
its ABI entirely.

See [VERSIONING.md](./VERSIONING.md) for bump rules and
[CHANGELOG.md](../CHANGELOG.md) for released versions.

## Shared machinery

Most contracts build on the same cross-cutting surface from `shared`:

| Concern | What it provides |
|---|---|
| Emergency stop | `pause` / `unpause`, plus a time-locked `schedule_recovery` that cannot be skipped by the admin |
| Health | `ok`/`error` counters, bps thresholds, stall detection, cooldown-limited `hlth_alrt` events |
| Rollout | Canary/stable traffic split with sticky SHA-256 routing, feature flags, and automatic rollback on error-rate breach |
| Versioning | Semver negotiation, min-compatible calculation, storage-schema constant |
| Correlation | Deterministic `CorrelationId` derivation and parent/child linking for indexers |
| Config | Typed cross-contract lookups against `platform_config` (`get_fee_bps`, `get_usdc`, `get_admin`, `get_platform_wallet`) |

Two things about that library are worth knowing before relying on it. First,
several of its setters (`set_alert_config`, `set_canary_deployment`,
`set_feature_flag`, `set_rollback_trigger`, `version::store`) perform **no
authorization of their own** — the consuming contract's wrapper is responsible
for calling `require_auth()` exactly once, or the transaction fails with a
double-auth `HostError`. Second, `upgrade::require_upgrade_safe` and
`export_storage_keys` are advertised in the module docs but are not implemented;
the real surface is `record_upgrade`, `get_version`, `require_paused_for_upgrade`
and `signal_migration_needed`.

---

## Contracts


### analytics

> Portfolio Analytics Contract — tracks artist performance metrics: earnings by category and client (#602), project completion rate, response time analytics, client satisfaction trends, and earnings predictions (rolling average). Closes #602.

Source: `contracts/analytics/`

**Purpose.**

`analytics` is the platform's read-side performance ledger for artists. A trusted platform oracle (the stored admin) reports each commission outcome to it — a payout with its category and client, a cancellation, a response latency, a 1–5★ satisfaction score — and the contract maintains per-artist aggregates plus a per-record earnings log supporting paginated drill-down. Derived read entry points (completion rate, average response time, average satisfaction, and a mean-payout earnings prediction) are computed on chain from those aggregates. It also carries a deliberately narrow, double-bounded retention/pruning path for the derived earnings log only. Unlike every other workspace contract, `analytics` does **not** depend on `shared`: there is no pause, health, rollout, or semver surface here, and its only authorization role is the single admin set at `initialize` (no admin rotation).

**Dependencies.**

Path deps: **none** — `shared` is deliberately absent. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with the `testutils` feature as a dev-dependency. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. No `[package.metadata.stellar-aid]` block, so this crate pins no declared `storage-schema` or `min-compatible`. `crate-type = ["cdylib", "rlib"]`.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), AnalyticsError>` | `admin.require_auth()` | `init` | Checks `instance().has(Admin)` **before** `require_auth()`, so a second call reports `AlreadyInitialized` rather than an auth failure. Single-shot; no admin rotation |
| `record_earning` | `record_earning(env: Env, artist: Address, commission_id: Bytes, category: String, client: Address, amount: i128) -> Result<(), AnalyticsError>` | `admin.require_auth()` (admin loaded from `instance`) | `earning` | Rejects `amount <= 0`; `checked_add` on `total_earnings` and `completed_count` ⇒ `ArithmeticOverflow`; appends at index `EarningCount` then increments |
| `record_cancellation` | `record_cancellation(env: Env, artist: Address) -> Result<(), AnalyticsError>` | `admin.require_auth()` | `cancel` | Increments `cancelled_count`; no amount validation |
| `record_response_time` | `record_response_time(env: Env, artist: Address, response_ledgers: u64) -> Result<(), AnalyticsError>` | `admin.require_auth()` | `resp_time` | Rejects `response_ledgers == 0` with `InvalidAmount` |
| `record_satisfaction` | `record_satisfaction(env: Env, artist: Address, score_x10: u32) -> Result<(), AnalyticsError>` | `admin.require_auth()` | `satisf` | Requires `10..=50` (1–5★ ×10), else `InvalidScore` |
| `get_metrics` | `get_metrics(env: Env, artist: Address) -> Result<ArtistMetrics, AnalyticsError>` | none (view) | — | `NotFound` when the aggregate was never created |
| `get_completion_rate` | `get_completion_rate(env: Env, artist: Address) -> Result<u32, AnalyticsError>` | none (view) | — | `completed / (completed + cancelled) * 100`, truncated; `0` when both are 0; `NotFound` when uninitialised |
| `get_avg_response_time` | `get_avg_response_time(env: Env, artist: Address) -> Result<u64, AnalyticsError>` | none (view) | — | Floor division; `0` when no samples |
| `get_avg_satisfaction` | `get_avg_satisfaction(env: Env, artist: Address) -> Result<u32, AnalyticsError>` | none (view) | — | Floor division of the ×10 sums; `0` when no samples |
| `predict_earnings` | `predict_earnings(env: Env, artist: Address) -> Result<i128, AnalyticsError>` | none (view) | — | Mean payout `total_earnings / completed_count`; `0` when `completed_count == 0` |
| `get_earning` | `get_earning(env: Env, artist: Address, index: u32) -> Result<EarningsRecord, AnalyticsError>` | none (view) | — | Single record by sequential index from 0; also returns `NotFound` for a pruned index |
| `get_earning_count` | `get_earning_count(env: Env, artist: Address) -> u32` | none (view) | — | Infallible; defaults to `0`. Not decremented by pruning |
| `get_earnings` | `get_earnings(env: Env, artist: Address, from_index: u32, limit: u32) -> Result<soroban_sdk::Vec<EarningsRecord>, AnalyticsError>` | none (view) | — | Reads `[from_index, from_index + min(limit, 50))`; silently skips pruned holes, so a page may be short; `from_index` past the end yields an empty page rather than an error |
| `prune_earnings` | `prune_earnings(env: Env, artist: Address, upto_index: u32, limit: u32) -> Result<u32, AnalyticsError>` | `admin.require_auth()` | `prune` | Requires `upto_index != 0` and `1..=100`, else `InvalidAmount`; scans from index `0` and **breaks at the first record newer than the cutoff** so a prune cannot reach past protected data; `limit` bounds both removals and indexes visited; returns the number removed |
| `get_retention_policy` | `get_retention_policy(env: Env) -> (u32, u32)` | none (view) | — | Returns `(MIN_RETENTION_LEDGERS, PRUNE_MAX_BATCH)`; `env` is explicitly discarded |

**Events.**

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `(admin,)` | `initialize` |
| `earning` | `(symbol_short!("earning"),)` | `(artist: Address, commission_id: Bytes, category: String, amount: i128)` — note `client` is *not* in the payload | `record_earning` |
| `cancel` | `(symbol_short!("cancel"),)` | `(artist,)` | `record_cancellation` |
| `resp_time` | `(symbol_short!("resp_time"),)` | `(artist: Address, response_ledgers: u64)` | `record_response_time` |
| `satisf` | `(symbol_short!("satisf"),)` | `(artist: Address, score_x10: u32)` | `record_satisfaction` |
| `prune` | `(symbol_short!("prune"),)` | `(artist: Address, removed: u32, scanned: u32, cutoff: u32)` | `prune_earnings` |

**Errors.**

`errors::AnalyticsError`, `#[contracterror]`, `#[repr(u32)]`.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` found `DataKey::Admin` already present in instance storage |
| `NotInitialized` | 2 | `require_admin` could not read `DataKey::Admin` — i.e. a writer ran before `initialize` |
| `Unauthorized` | 3 | **Never constructed.** A non-admin caller fails at `admin.require_auth()` with a host auth error, not this variant; the variant is only mapped in `Display` and `get_suggestion` (`AUTH`) |
| `InvalidAmount` | 4 | `record_earning` with `amount <= 0`; `record_response_time` with `response_ledgers == 0`; `prune_earnings` with `upto_index == 0`, `limit == 0`, or `limit > 100` |
| `NotFound` | 5 | No `DataKey::Metrics` / `DataKey::Earning` entry for the artist or index |
| `ArithmeticOverflow` | 6 | `checked_add` overflowed on `total_earnings`, `completed_count`, `cancelled_count`, `response_time_sum`, `response_time_count`, `satisfaction_score_sum`, or `satisfaction_score_count` |
| `InvalidScore` | 7 | `score_x10` outside `10..=50` |

**Storage.** See [STORAGE.md](./STORAGE.md#analytics).

**Compile status.** `compiles` — `cargo check -p analytics --lib` succeeds.


### audit

> Transaction History and Audit Log Contract — closes #712, with immutability achieved structurally (append-only under a monotonic sequence number, no `update_entry`/`delete_entry`/`clear` anywhere), all reads bounded by `MAX_PAGE_SIZE`/`MAX_QUERY_SCAN`, and no free text, memos, names, or identifiers of any kind in the stored schema.

Source: `contracts/audit/`

**Purpose.**

`audit` is the workspace's tamper-evident history layer, giving auditors, indexers, and dispute reviewers a verifiable recent window of who moved what, when, to whom, and in what state. Immutability is structural rather than a flag: every append is written under a sequence number drawn from a counter that only increases, and the crate exposes no remove, setter, or clear path for entry keys — the absence *is* the guarantee, so a status change is a further append under the same `reference` rather than an edit. Two append paths exist: an admin-only `record_transaction` for value movements (which must name a token) and a self-attested `log_activity` whose kind, status, amount, and token are all fixed inside the function so an activity marker can never be shaped like a settlement. It then layers the `shared` health and gradual-rollout surface on top (`health_check` auto-disables the canary on an anomaly) and splices in its own Cargo.toml-only semver queries via `include!`. It does **not** expose `shared::pause` or `shared::upgrade` entry points — no `pause`/`unpause`, no `record_upgrade` — even though `trigger_rollback` does set the shared pause flag.

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`). Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with `testutils` as a dev-dependency. `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. Note that the three semver entry points come from `include!("../../semver_types.rs")` — the Cargo.toml-only twin in `contracts/semver_types.rs`, *not* from `shared::version::impl_semver_queries!` — so they read no storage and ignore `UpgradeKey::Version`. `pub use storage::DataKey as StorageKey;` re-exports the key enum so the immutability claim can be audited externally.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), AuditError>` | `admin.require_auth()` | `aud_init` | `require_auth()` runs **before** the `is_initialized` check, unlike `analytics` |
| `get_version` | `get_version(env: Env) -> ContractVersion` | none (view) | — | From `impl_semver_queries!()`; parses `CARGO_PKG_VERSION` only — no storage read |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Adds `min_compatible` and `storage_schema` (`CURRENT_STORAGE_SCHEMA = 1`) |
| `is_version_compatible` | `is_version_compatible(env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Major must match; on `0.x` the minor must match and only patch may drift; on `>=1.0` the required tuple must be `<=` the crate version |
| `record_transaction` | `record_transaction(env: Env, reference: Bytes, action: AuditAction, from: Address, to: Address, amount: i128, token: Address, status: TxStatus) -> Result<u32, AuditError>` | `admin.require_auth()` (via `require_admin`, which also returns `NotInitialized`) | `aud_txn` | Rejects `amount < 0`; `token` is a non-`Option` parameter, so a `UserActivity` action here is rejected by `build_entry` as `InvalidAction`; returns the new sequence number |
| `log_activity` | `log_activity(env: Env, actor: Address, reference: Bytes) -> Result<u32, AuditError>` | `actor.require_auth()` (self-attestation) | `aud_act` | `from == to == actor`, `amount = 0`, `token = None`, `action = UserActivity`, `status = Activity` — all fixed in the body, so an account can only log for itself and cannot forge a value-settlement-shaped record |
| `get_entry_count` | `get_entry_count(env: Env) -> u32` | none (view) | — | Infallible; `storage::next_sequence`, i.e. total appends and the next sequence number |
| `get_entry` | `get_entry(env: Env, sequence: u32) -> Result<AuditEntry, AuditError>` | none (view) | — | `EntryNotFound` for an unknown **or TTL-expired** sequence |
| `get_transaction` | `get_transaction(env: Env, reference: Bytes) -> Result<AuditEntry, AuditError>` | none (view) | — | Newest entry for a reference (index `count - 1`); `ReferenceNotFound` when `count == 0` |
| `get_transaction_history` | `get_transaction_history(env: Env, reference: Bytes, page: u32, page_size: u32) -> Result<AuditPage, AuditError>` | none (view) | — | Index-backed walk; never `truncated`, `next_sequence = end` when `end < total` |
| `get_by_account` | `get_by_account(env: Env, account: Address, page: u32, page_size: u32) -> Result<AuditPage, AuditError>` | none (view) | — | Index-backed; an account sees its entries as sender and, when different, as recipient |
| `get_by_status` | `get_by_status(env: Env, status: TxStatus, page: u32, page_size: u32) -> Result<AuditPage, AuditError>` | none (view) | — | Index-backed |
| `get_by_ledger_range` | `get_by_ledger_range(env: Env, from_ledger: u32, to_ledger: u32, start_sequence: u32, page: u32, page_size: u32) -> Result<AuditPage, AuditError>` | none (view) | — | Unindexed forward scan capped at `MAX_QUERY_SCAN` (200) visited entries; `InvalidLedgerRange` when `from_ledger > to_ledger`; sets `truncated` and returns a `next_sequence` cursor when the bound is hit |
| `export_audit_log` | `export_audit_log(env: Env, from_ledger: u32, to_ledger: u32, start_sequence: u32, page: u32, page_size: u32) -> Result<AuditExport, AuditError>` | none (view) | — | Same bounded walk as `get_by_ledger_range` but reporting-shaped; the `next_sequence` cursor is computed and then discarded (`_`), so the caller must re-derive its own resume point |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt`, and `rollback` if an anomaly auto-rolls back | Delegates to `shared::health::health_check`, then calls `shared::rollout::maybe_auto_rollback` when `report.anomaly` |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | `env` discarded; pure constant set |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` (caller-supplied `admin` param, not the stored one) | `alrt_cfg` | **This is the wrapper's own `require_auth`, not `shared::health`'s** — the shared function has no auth check. `admin` is not checked against `DataKey::Admin`, so any address may set thresholds here |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` (caller-supplied) | — | Wrapper-only guard; `shared::health::record_ok` performs no auth |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` (caller-supplied) | — | As above |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool)` | `admin.require_auth()` (caller-supplied) | `feat_flg` | |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool` | none (view) | — | Forced `false` while `Phase == RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` (caller-supplied) | `canary` | `shared::rollout` panics above 10,000 bps |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky SHA-256 routing |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` (caller-supplied) | `rb_trig` | |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` (caller-supplied) | `rollback`, `contract_paused` | `admin.require_auth()` is called once here so `shared::rollout::trigger_rollback` can avoid a double-auth `HostError`; zeroes canary bps, sets `RolledBack`, disables all flags, and sets the shared pause flag |

**Events.**

`audit`'s own three shapes, then the eight it inherits from the `shared` helpers its entry points call. Note the two distinct spellings of the same name in the `shared` layer: long names use `Symbol::new`, short ones `symbol_short!`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `aud_init` | `(symbol_short!("aud_init"),)` | `admin` (bare `Address`, not a tuple) | `initialize` |
| `aud_txn` | `(symbol_short!("aud_txn"),)` | `(reference: Bytes, sequence: u32, action: AuditAction, status: TxStatus, amount: i128)` | `record_transaction` |
| `aud_act` | `(symbol_short!("aud_act"),)` | `(reference: Bytes, sequence: u32, actor: Address)` | `log_activity` |
| `contract_paused` | `(Symbol::new(env, "contract_paused"),)` | `shared::pause::ContractPausedEvent { admin }` | `trigger_rollback` (via `shared::rollout::trigger_rollback`) |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `config.unhealthy_error_bps` (bare `u32`) | `set_alert_config` (via `shared::health::set_alert_config`) |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check` (via `shared::health::health_check`) |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment` (via `shared::rollout::set_canary_deployment`) |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag` (via `shared::rollout::set_feature_flag`) |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `error_bps` (bare `u32`) | `set_rollback_trigger` (via `shared::rollout::set_rollback_trigger`) |
| `rollback` | `(symbol_short!("rollback"),)` | `env.ledger().sequence()` (bare `u32`) | `trigger_rollback`, and `health_check` when `maybe_auto_rollback` fires |

**Errors.**

`errors::AuditError`, `#[contracterror]`.

| Variant | Code | Condition |
|---|---|---|
| `NotInitialized` | 1 | `require_admin` found no `DataKey::Admin`, so nothing may be appended |
| `AlreadyInitialized` | 2 | `initialize` ran twice (the `require_auth` on the second call still executes first) |
| `Unauthorized` | 3 | **Never constructed by any entry point.** A non-admin caller fails at `admin.require_auth()` with a host auth error. The variant is referenced only in `Display`, in `get_suggestion` (`AUTH`), and in `src/test.rs:535` which asserts its numeric code is `3` |
| `EntryNotFound` | 4 | No entry for the sequence number — including after TTL expiry, since `persistent` entries vanish at their TTL |
| `ReferenceNotFound` | 5 | `get_transaction` found `reference_count == 0` |
| `InvalidAmount` | 6 | `build_entry` was given `amount < 0` |
| `InvalidAction` | 7 | Action and token disagree on entry kind: `UserActivity` with `Some(token)`, or any other action with `None` |
| `InvalidPageSize` | 8 | `page_size == 0` or `page_size > MAX_PAGE_SIZE` (50), checked by `check_page` on every paged query |
| `InvalidLedgerRange` | 9 | `from_ledger > to_ledger` in `get_by_ledger_range` or `export_audit_log` |

**Storage.** See [STORAGE.md](./STORAGE.md#audit).

**Compile status.** `compiles` — `cargo check -p audit --lib` succeeds. Two behavioural notes rather than build errors: the 19 `shared`-backed entry points authorize a caller-supplied `admin` parameter without comparing it to the stored `DataKey::Admin`, so any address can mutate health/rollout configuration; and `AuditError::Unauthorized` is unreachable in the current implementation.


### campaign

> unknown — `contracts/campaign/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc comment; the summary below is reconstructed from the source (and matches `docs/CONTRACTS.md`: "Manages fundraising campaign lifecycle"). `src/invariant_tests.rs` does carry a `//!` doc, but it describes the test invariants, not the contract.

Source: `contracts/campaign/`

**Purpose.**

`campaign` is the on-chain registry of fundraising campaigns: it allocates campaign IDs, stores each campaign's `shared::types::Campaign` record (owner, goal, `raised`, `CampaignStatus`, deadline, `fee_bps`, `platform_wallet`), and tracks the aggregate raised total that other contracts push into it. It is the *accounting and status* surface for a campaign, deliberately not the money-movement surface — the Donation contract moves tokens and calls back into `update_raised` / reads `get_campaign` cross-contract, and the Withdrawal contract keeps its own per-campaign ledger. It gives the platform admin a full operational toolbox over that registry: a hard emergency freeze, a softer pause, dual-authorized admin rotation, a manual fraud-review flag that blocks withdrawals only, an audit trail, and archive/delete of finished campaigns. Finally, it carries the shared health-monitoring (#678) and gradual-rollout (#684) surface, so operators can watch it and shift traffic between canary/stable WASM builds from the same contract.

**Dependencies.**

Path dep: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — used for `shared::types::{Campaign, CampaignStatus}`, `shared::pause`, `shared::health`, `shared::rollout`, `shared::version`, and `shared::upgrade::ContractVersion`. Registry dep: `soroban-sdk` `21.0.0` (workspace-inherited; dev-dependency adds the `testutils` feature). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `[lib] crate-type = ["cdylib"]` (no `rlib`). Note: the root `Cargo.toml` `NOTE` claims `contracts/campaign` "exists on disk but [is] still not a workspace member, so [it is] never built" — that comment is **stale**: `contracts/campaign` is listed in `[workspace] members` (line 26) and `cargo check -p campaign` builds it.

**Public interface.**

45 `pub fn`s in the single `#[contractimpl] impl CampaignContract` block. The private `fn`s — `check_frozen`, `require_not_frozen`, `check_under_review`, `require_not_under_review`, `ensure_admin`, `next_campaign_id` — are internal and omitted. Note the `#[contract] MockMultiSigWallet` + `#[contractimpl]` at lines 1008–1021 live inside `#[cfg(test)] mod test` and are a test fixture, not part of this contract's interface.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address)` | `admin.require_auth()` | — | One-shot: aborts `"already initialized"` if `DataKey::Initialized` is set. Seeds `Admin`, `Initialized`, `CampaignCount = 0` and `shared::version::seed`. Any address may initialize an uninitialized contract |
| `get_version` | `get_version(env: Env) -> shared::upgrade::ContractVersion` | none (view) | — | Reads `UpgradeKey::Version`, falls back to `CARGO_PKG_VERSION` |
| `get_version_metadata` | `get_version_metadata(env: Env) -> shared::version::VersionMetadata` | none (view) | — | Name, semver, min-compatible client, `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Major must match; on `0.x` the minor must also match |
| `freeze` | `freeze(env: Env)` | `admin.require_auth()` — the `Address` read from `DataKey::Admin` | `contract_frozen` | Admin-rotation path: the admin is *not* a parameter, so the caller is whichever address currently sits in `DataKey::Admin`. Blocks all 14 `require_not_frozen` sites, but leaves views readable. Expected caller is a multi-sig contract account (see `docs/ADMIN_MULTISIG.md`; the `MockMultiSigWallet` test fixture exercises exactly this) |
| `unfreeze` | `unfreeze(env: Env)` | `admin.require_auth()` — read from `DataKey::Admin` | `contract_unfrozen` | Sets `Frozen = false`; same multi-sig-expected caller as `freeze`. Does not touch the pause flag |
| `is_frozen` | `is_frozen(env: Env) -> bool` | none (view) | — | Reads `DataKey::Frozen`, default `false` |
| `flag_for_review` | `flag_for_review(env: Env, reason_hash: BytesN<32>)` | `admin.require_auth()` — read from `DataKey::Admin` | `flag_for_review` | Fraud-review path. Sets `UnderReview = true` and stores `reason_hash`; the doc comment says the automated Phase-4 AI fraud scorer is out of scope and will reach this hook through an authorized off-chain service/oracle, so today only the admin can set it |
| `clear_review_flag` | `clear_review_flag(env: Env)` | `admin.require_auth()` — read from `DataKey::Admin` | `clear_review_flag` | Sets `UnderReview = false` and `remove`s `ReviewReason`. Under review blocks **only** `finalize_withdrawal`; reads, `update_raised` and status changes keep working |
| `is_under_review` | `is_under_review(env: Env) -> bool` | none (view) | — | Reads `DataKey::UnderReview`, default `false` |
| `get_review_reason` | `get_review_reason(env: Env) -> Option<BytesN<32>>` | none (view) | — | `None` when no review is armed |
| `pause` | `pause(env: Env, admin: Address)` | `admin.require_auth()` + `admin` must equal `DataKey::Admin` (`ensure_admin`, else `"unauthorized"`) | `contract_paused` | Delegates to `shared::pause::pause`; also zeroes `PauseDataKey::RecoveryEta`. Frozen blocks it |
| `unpause` | `unpause(env: Env, admin: Address)` | `admin.require_auth()` + `ensure_admin` | `contract_unpaused` | Delegates to `shared::pause::unpause`; since this contract never arms `RecoveryEta`, the #711 time-lock never rejects it. `recovery_completed` is therefore unreachable here |
| `create_campaign` | `create_campaign(env: Env, owner: Address, goal: i128, deadline: u64, fee_bps: u32, platform_wallet: Option<Address>) -> u64` | `owner.require_auth()` | `campaign_registered` | Returns the new 1-based ID. Aborts if `fee_bps > 1000`, if `deadline > now + 63_115_200` (~2 years), or if `deadline <= now`; stores with `status = Active`, `raised = 0`, then bumps TTL |
| `get_campaign` | `get_campaign(env: Env, campaign_id: u64) -> Option<Campaign>` | none (view) | — | `None` for an unknown or archived ID |
| `update_campaign_status` | `update_campaign_status(env: Env, admin: Address, campaign_id: u64, new_status: CampaignStatus)` | `admin.require_auth()` + `ensure_admin` | `campaign_status_changed` | Generic status transition; `unwrap()`s the campaign so an unknown ID panics. Does **not** call `pause::require_not_paused` (unlike `reject_campaign`) |
| `update_raised` | `update_raised(env: Env, campaign_id: u64, amount: i128)` | **none** — permissionless, no `require_auth` | — | **Cross-contract entry point**: the Donation contract (`contracts/donation`, via `#[contractclient(name = "CampaignContractClient")]`) calls this after a successful donation, and Donation's `refund` decrements its own mirror key rather than calling back. `campaign.raised += amount` with a plain (overflow-checked) add, then TTL bump; `unwrap()`s the campaign |
| `finalize_withdrawal` | `finalize_withdrawal(env: Env, campaign_id: u64, amount: i128)` | `campaign.owner.require_auth()` — the owner read from the stored record | `withdrawal_finalized` | The only path the fraud-review flag gates: aborts `UnderReview` when flagged, `ContractFrozen` when frozen, and on `pause::require_not_paused`. Aborts if `amount <= 0` or `amount > campaign.raised`. Moves no tokens — it only debits the `raised` accounting field; the Withdrawal contract is the component that actually pays out |
| `approve_campaign` | `approve_campaign(env: Env, admin: Address, campaign_id: u64)` | `admin.require_auth()` + `ensure_admin` (via `update_campaign_status`) | `campaign_status_changed` | Thin wrapper that sets `CampaignStatus::Active` |
| `reject_campaign` | `reject_campaign(env: Env, admin: Address, campaign_id: u64, reason: String)` | `admin.require_auth()` + `ensure_admin` | `campaign_status_changed` | Sets `CampaignStatus::Rejected`; aborts if `reason.len() > 512` (closes #591). **`reason` is validated then discarded** (`let _ = reason;`) — the rejection rationale is not persisted or emitted. Also the only status path that checks `pause::require_not_paused` |
| `suspend_campaign` | `suspend_campaign(env: Env, admin: Address, campaign_id: u64)` | `admin.require_auth()` + `ensure_admin` (via `update_campaign_status`) | `campaign_status_changed` | Thin wrapper that sets `CampaignStatus::Suspended` |
| `get_campaign_count` | `get_campaign_count(env: Env) -> u64` | none (view) | — | Total campaigns ever created; never decremented by `archive_campaign` |
| `get_admin` | `get_admin(env: Env) -> Address` | none (view) | — | The current admin; aborts `"contract not initialized"` if unset |
| `set_admin` | `set_admin(env: Env, new_admin: Address)` | **dual**: `current_admin.require_auth()` **and** `new_admin.require_auth()` | `admin_changed` | The admin-rotation path. Payload is `AdminChangedEvent { old_admin, new_admin }`; exactly one admin is kept. `require_not_frozen` + `pause::require_not_paused` both gate it. Works with a native Stellar multi-sig account (per the `set_admin_requires_both_auths` / `set_admin_successful_rotation_and_single_admin` / `test_admin_multisig_*` tests) |
| `transfer_admin` | `transfer_admin(env: Env, current_admin: Address, new_admin: Address)` | `current_admin.require_auth()` + `ensure_admin` | — | A **one-sided** rotation: `new_admin` never authorizes, so a compromised current admin can hand the role to an attacker address without its key. Exists alongside the safer `set_admin` and emits no event. Frozen and paused both block it |
| `upgrade` | `upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>)` | `admin.require_auth()` + `ensure_admin` | — | Calls the host `env.deployer().update_current_contract_wasm(new_wasm_hash)`. Deliberately bypasses `shared::upgrade`, so it does **not** require the contract to be paused, does **not** write `UpgradeKey::Version` / `LastUpgradeLedger`, and emits no `upgraded` event. This is not a cross-contract call |
| `archive_campaign` | `archive_campaign(env: Env, admin: Address, campaign_id: u64)` | `admin.require_auth()` + `ensure_admin` | `campaign_archived` | Deletes the persistent record; aborts `"cannot archive an active or pending campaign"` for `Active`/`Pending`, i.e. any campaign with funds possibly in flight. `unwrap()`s the campaign |
| `get_fee_config` | `get_fee_config(env: Env, campaign_id: u64) -> (u32, Option<Address>)` | none (view) | — | Returns `(campaign.fee_bps, campaign.platform_wallet)`; `unwrap()`s, so an unknown ID panics rather than returning `None` |
| `bump_campaign_ttl` | `bump_campaign_ttl(env: Env, campaign_id: u64)` | **none** — permissionless, no `require_auth` | — | `extend_ttl(&Campaign(id), MIN_TTL, MAX_TTL)`. Deliberately exposed so anyone (a keeper, or the Donation contract) can keep a live campaign's entry alive; the only way `MIN_TTL`/`MAX_TTL` are ever applied |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none | `hlth_alrt` (only when an anomaly fires and the cooldown has elapsed), `rollback` (only via auto-rollback) | **Not a pure view**: it writes `HealthKey::LastAlertLedger` and, when `report.anomaly` is true, calls `shared::rollout::maybe_auto_rollback` which zeroes `CanaryBps`, sets `Phase = RolledBack` and disables all flags. It is also the one health/rollout path with **no** `require_not_frozen` guard |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | Mirrors `PauseDataKey::Paused` into `metrics.paused` |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Returns the compile-time SLA constants; `env` is unused |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` — **caller-supplied and never compared to `DataKey::Admin`** | `alrt_cfg` | Because there is no `ensure_admin`, *any* address can sign for itself and rewrite the alerting thresholds. `shared::health::set_alert_config` aborts `"invalid alert config"` on inverted bps, `> 10000`, or a zero stall/cooldown window |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to `default_alert_config()` |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | True when degraded, unhealthy, or stalled |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | Increments `ok_count` and stamps `last_ok_ledger`. Fed by operators, not the contract's own entry points, so counters only move when someone reports |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | Increments `error_count` and stamps `last_error_ledger` |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `feat_flg` | Writes `RolloutKey::Flag(flag)` and appends to `FlagIndex` if new. No `ensure_admin` |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | Always `false` once the phase is `RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `canary` | Aborts `"canary_bps exceeds 10000"`; derives `Phase` from the bps. No `ensure_admin` |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | none | — | Sticky `SHA-256(caller XDR) mod 10000 < canary_bps` split; the passed `caller` is unauthenticated, so the answer is advisory only |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | `{phase, canary_bps, canary, stable, rollback_error_bps, flag_count}` |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rb_trig` | Aborts `"invalid rollback trigger"` when `error_bps == 0` or `> 10000`. No `ensure_admin` |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | Compares `health::error_bps` against `RollbackErrorBps` |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rollback`, `contract_paused` | Zeroes canary traffic, sets `Phase = RolledBack`, disables every indexed flag, **and sets `PauseDataKey::Paused = true`** — so any address that signs for itself can pause the campaign contract through this door. No `ensure_admin` |

**Events.**

18 distinct shapes. Nine are contract-specific (long symbols via `Symbol::new`); nine more arrive from the delegated `shared` helpers, one of which (`recovery_completed`) is unreachable from this contract. Deduped: `campaign_status_changed` is published from four different functions (two payload constructions), and `contract_paused` is published both by `pause` and by `trigger_rollback`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `contract_frozen` | `(Symbol::new(&env, "contract_frozen"),)` | `ContractFrozenEvent { admin: Address }` | `freeze` |
| `contract_unfrozen` | `(Symbol::new(&env, "contract_unfrozen"),)` | `ContractUnfrozenEvent { admin: Address }` | `unfreeze` |
| `admin_changed` | `(Symbol::new(&env, "admin_changed"),)` | `AdminChangedEvent { old_admin: Address, new_admin: Address }` | `set_admin` only — `transfer_admin` rotates silently |
| `flag_for_review` | `(Symbol::new(&env, "flag_for_review"),)` | `CampaignUnderReviewEvent { admin: Address, reason_hash: BytesN<32> }` | `flag_for_review` |
| `clear_review_flag` | `(Symbol::new(&env, "clear_review_flag"),)` | `CampaignReviewClearedEvent { admin: Address }` | `clear_review_flag` — the stored `reason_hash` is **not** in the payload |
| `campaign_registered` | `(Symbol::new(&env, "campaign_registered"),)` | `CampaignRegisteredEvent { campaign_id: u64, owner: Address, goal: i128, deadline: u64 }` | `create_campaign` — `fee_bps` and `platform_wallet` are omitted |
| `campaign_status_changed` | `(Symbol::new(&env, "campaign_status_changed"),)` | `CampaignStatusChangedEvent { campaign_id: u64, old_status: CampaignStatus, new_status: CampaignStatus }` | `update_campaign_status` (any target status), plus `approve_campaign` (→ `Active`), `reject_campaign` (→ `Rejected`) and `suspend_campaign` (→ `Suspended`) |
| `withdrawal_finalized` | `(Symbol::new(&env, "withdrawal_finalized"),)` | `(campaign_id: u64, amount: i128)` — a bare tuple, not a `#[contracttype]` | `finalize_withdrawal` |
| `campaign_archived` | `(Symbol::new(&env, "campaign_archived"),)` | `(campaign_id: u64)` — a bare one-element tuple | `archive_campaign` |
| `contract_paused` | `(Symbol::new(&env, "contract_paused"),)` | `shared::pause::ContractPausedEvent { admin: Address }` | `pause`, and `trigger_rollback` |
| `contract_unpaused` | `(Symbol::new(&env, "contract_unpaused"),)` | `shared::pause::ContractUnpausedEvent { admin: Address }` | `unpause` |
| `recovery_completed` | `(Symbol::new(&env, "recovery_completed"),)` | `shared::pause::RecoveryCompletedEvent { admin: Address, eta_ledger: u32 }` | **Unreachable in this crate** — emitted by `shared::pause::try_unpause` only when a `RecoveryEta` matured, and `campaign` exposes no `schedule_recovery` |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check`, rate-limited by `AlertConfig::alert_cooldown_ledgers` |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `config.unhealthy_error_bps: u32` — a single value | `set_alert_config` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `error_bps: u32` — a single value | `set_rollback_trigger` |
| `rollback` | `(symbol_short!("rollback"),)` | `env.ledger().sequence(): u32` — a single value | `trigger_rollback`, and `health_check` via `maybe_auto_rollback` |

**Errors.**

None. `campaign` declares no `#[contracterror]` enum anywhere in the crate — there is no `src/errors.rs`. Every failure is a bare `panic!` with a string literal, so callers get an opaque `HostError` and cannot branch on a code: `"already initialized"`, `"contract not initialized"`, `"fee_bps must not exceed 1000"`, `"deadline arithmetic overflow"`, `"deadline exceeds maximum allowed (2 years from now)"`, `"deadline must be in the future"`, `"withdrawal amount must be positive"`, `"insufficient funds: requested exceeds raised amount"`, `"reason exceeds maximum allowed length"`, `"cannot archive an active or pending campaign"`, `"unauthorized"` (from `ensure_admin`), plus `unwrap`/`expect` aborts: `"ContractFrozen"`, `"UnderReview"`, and `shared::pause::require_not_paused`'s `PauseError::ContractPaused` description. `CampaignStatus` and `shared::pause::PauseError` are the only error-like enums reachable, and neither is a `#[contracterror]` owned by this crate.

| Variant | Code | Condition |
|---|---|---|
| none | — | unknown — the crate has no `#[contracterror]` enum; all failures are string-literal `panic!`/`unwrap` aborts listed above |

**Storage.** See [STORAGE.md](./STORAGE.md#campaign).

**Compile status.** `compiles` — `cargo check -p campaign` and `cargo test -p campaign --no-run` both succeed with no warnings. The 16 unit tests in the inline `#[cfg(test)] mod test` plus the 8 in `src/invariant_tests.rs` compile. It *is* a workspace member despite the stale `NOTE` in the root `Cargo.toml`.


### commission_agreement

> CommissionAgreement contract — core agreement lifecycle functions.

Source: `contracts/commission_agreement/`

**Purpose.**

This is the workspace's largest contract and the bookkeeping core of a two-party creative commission marketplace: it records the terms of a commission, walks the agreement through `Pending → Active → Completed`/`Cancelled`, and splits the agreed `budget_usdc` into milestones that the client approves one at a time. It solves the problem that an artist and a client need a shared, auditable, on-chain record of *what was agreed, what has been delivered, and what either side still owes* — without any party being able to move the other's funds. Fund custody itself is deliberately out of scope: revisions and cancellations only adjust the agreed `budget_usdc` figure and produce a `CancellationQuote` whose two halves always sum to the budget, leaving the actual token movement to the separate escrow contract. Around that core it layers team collaboration (invite/accept/decline contributors with a basis-point payment split), commission revisions with a bounded negotiation history, pro-rata cancellation with a grace window and a walking-party penalty, and an agency layer (artist rosters, revenue splits, batch payouts) that routes commissions and earnings through a representing agency. It also re-exports the `shared` crate's health-monitoring and gradual-rollout machinery as contract endpoints.

**Dependencies.**

| Dependency | Kind | Version | Notes |
|---|---|---|---|
| `soroban-sdk` | registry (workspace) | `21.0.0` (workspace declaration) | Resolved to `21.7.7` in the build. Provides `Env`, `Address`, `Bytes`, `String`, `Vec`, `Symbol`, `token::Client`, `#[contract]`, `#[contractimpl]`, `#[contracttype]`, `#[contracterror]`, `symbol_short!`. |
| `shared` | path dep | `../shared`, version `0.1.0` | Workspace sibling contract crate. Used for the 17 health/rollout wrapper functions via `shared::health::{…}` and `shared::rollout::{…}`, and for the `shared::health::{HealthReport, HealthMetrics, SlaTargets, AlertConfig}` / `shared::rollout::RolloutState` types that appear in the contract's public signatures. |
| `soroban-sdk` (dev-dependency) | registry (workspace) | workspace `21.0.0`, `features = ["testutils"]` | Test-only; re-declared in `[dev-dependencies]` with the `testutils` feature. |

Crate features: `default = []`; `testutils = ["soroban-sdk/testutils"]`; `legacy_tests = []` (an empty marker feature that gates the four stale test modules — see Modules). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. No `no_std`-unsafe, no extra registry deps (`serde`, `thiserror`, `stellar-xdr` etc. are not used directly).

**Modules.**

| Module | Responsibility |
|---|---|
| `src/lib.rs` (1335 lines) | Crate root and the whole contract surface: module declarations, the `#[contract] pub struct CommissionAgreementContract`, the single `#[contractimpl] impl` block with all 51 explicit `pub fn` entry points, `include!("../../semver_types.rs")`, free helper functions (`load_agreement`, `approved_total`, `load_policy`, `load_revision_policy`, `load_revisions`, `save_revisions`, `load_agency`, `load_roster_entry`, `load_analytics`, `save_analytics`, `attribute_commission`), the input-length and deadline constants, and a second plain `impl` block holding the private `in_grace` helper. |
| `src/types.rs` (110) | `#[contracttype]` data model: `AgreementStatus`, `MilestoneStatus`, `TeamRole`, `InvitationStatus`, the `TeamMember`, `AgreementRecord` and `MilestoneRecord` structs, and the `DataKey` storage-key enum (15 variants). |
| `src/errors.rs` (119) | The `#[contracterror]` `AgreementError` enum (30 variants, `#[repr(u32)]`), a hand-written `core::fmt::Display` impl giving each variant a message, and `get_suggestion` mapping each variant to a short `Symbol` remediation hint. |
| `src/agency.rs` (78) | Agency-layer types and pure maths: `AgencyProfile`, `RosterEntry`, `AgencyAnalytics`, `BatchPayment`; the constants `TOTAL_BPS = 10_000` and `MAX_BATCH = 25`; and the helpers `validate_split_bps` and `split_payment`. |
| `src/cancellation.rs` (175) | Cancellation types and the pro-rata settlement engine: `CancellationReason`, `Party`, `CancellationPolicy`, `CancellationQuote`, `CancellationRecord`; `TOTAL_BPS`, `DEFAULT_PENALTY_BPS = 1_000`; and `default_policy`, `validate_policy`, `completion_bps`, `settle`. |
| `src/revision.rs` (70) | Revision-negotiation types and limits: `RevisionStatus`, `RevisionRequest`, `RevisionPolicy`; `DEFAULT_MAX_REVISIONS = 5`, `MAX_REVISIONS_CAP = 50`; and `default_policy`, `validate_policy`. |
| `src/agency_tests.rs` (373) | `#[cfg(test)]`. Agency roster, split-bps bounds, single-representation, batch distribution and analytics roll-up tests, built on a local `Fixture` struct. Compiled by default. |
| `src/cancellation_tests.rs` (386) | `#[cfg(test)]`. Default/override policy, grace window, per-reason penalty placement, pro-rata quotes, event assertion, party-only cancellation, and the "settlement always sums to the budget" property test. Compiled by default. |
| `src/revision_tests.rs` (341) | `#[cfg(test)]`. Revision request/response, budget adjustment in both directions, self-approval refusal, stranger refusal, limit enforcement, and pending-state guards. Compiled by default. |
| `src/test.rs` (662) | Gated on `#[cfg(all(test, feature = "legacy_tests"))]`. Top-level `#![cfg(test)]` unit tests for `create_agreement`/`accept_agreement`/`reject_agreement`/`propose_milestone`/`approve_milestone`, the auth-negative cases, and the whole #603 team-colaboration suite. **Does not compile** with the feature on: it references `crate::CommissionAgreementContractClient` (a `#[contractimpl]`-generated type that needs a successful macro expansion, which the parse error prevents) and `create_agreement` needs a `DataKey::RateLimiter` instance entry that no test sets up. |
| `src/milestone_flow.rs` (145) | Gated on `legacy_tests`. Self-contained `mod tests` for a 3-milestone release flow. **Does not compile**: imports `crate::test::{helpers, test_lifecycles}` and `crate::types::{Client, Commission, Milestone}`, none of which exist any more, and defines `fn test_milestone_based_commission_flow` twice. |
| `src/multiple_escrows.rs` (147) | Gated on `legacy_tests`. Self-contained `mod tests` for two escrows in two different tokens. **Does not compile**: same dead `crate::test::…` / `crate::types::{Client, Commission, Milestone}` imports, and it additionally re-declares a second `#[contracterror] pub enum AgreementError` *and* a second `pub fn get_suggestion` inside the `mod tests` body, neither of which is imported (`Symbol`, `symbol_short!` are not in scope). |
| `src/dispute_resolution.rs` (76) | Gated on `legacy_tests`. Self-contained `mod tests` driving an external `dispute_arbiter` contract. **Does not compile**: dead `crate::test::…` imports plus a `dispute_arbiter::` cross-crate reference that is not in this crate's `Cargo.toml`. |
| `src/integration_tests.rs` (423) | Declared at `src/lib.rs:1335` but **not reachable**: that `mod` statement sits outside the unclosed `#[contractimpl]` block, so the parse error is reported there. Not compiled; contents not verified. |
| `contracts/semver_types.rs` (121, outside `src/`) | Not a module of its own — `include!`d verbatim into `lib.rs` at line 56. Defines `ContractVersion`, `VersionMetadata`, `CURRENT_STORAGE_SCHEMA`, `parse_pkg_semver`, `min_compatible_for`, `is_compatible`, and the `impl_semver_queries!` macro that injects three `pub fn`s into the `#[contractimpl]` block. |

**Public interface.**

All rows come from the single `#[contractimpl] impl CommissionAgreementContract` block at `src/lib.rs:186`. The first three rows are generated by the `impl_semver_queries!();` invocation at `src/lib.rs:187` (macro defined in `contracts/semver_types.rs`, spliced in via `include!` at `src/lib.rs:56`); all others are written literally in `lib.rs`. **Rows 12 and 13 onward are textually inside the unclosed `get_team_members` body** — see Compile status. Emitted events for the `shared`-delegating rows are published by the `shared` crate, not by this crate.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `get_version` | `(_env: Env) -> ContractVersion` | none (view) | — | Macro-generated (`semver_types.rs:89`). Parses `CARGO_PKG_VERSION`; ignores its arg. |
| `get_version_metadata` | `(env: Env) -> VersionMetadata` | none (view) | — | Macro-generated (`semver_types.rs:94`). Returns crate name, semver, `min_compatible_for(version)`, `storage_schema = CURRENT_STORAGE_SCHEMA` (1). |
| `is_version_compatible` | `(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated (`semver_types.rs:105`). Compares against the crate's own Cargo.toml version. |
| `create_agreement` | `(env, commission_id: Bytes, client: Address, artist: Address, title: String, budget_usdc: i128, deadline_ledger: u32) -> Result<(), AgreementError>` | `client.require_auth()` (the `client` argument) | `agr_crtd` (topics: `(agr_crtd)`, `(agr_new)`; data: `commission_id, client, artist, budget_usdc`) | Rate-limits via an `invoke_contract` of `check_rate_limit` on the `DataKey::RateLimiter` address, then validates `MAX_ID_LEN`/`MAX_TITLE_LEN`, `budget_usdc > 0`, deadline in `(now, now + MAX_DEADLINE_OFFSET_LEDGERS]`, and non-duplication; writes `Agreement` + empty `MilestonesForAgreement`; calls `attribute_commission`. Errors: `RateLimiterNotConfigured`, `InputTooLong`, `InvalidAmount`, `DeadlineInPast`, `ArithmeticOverflow`, `DeadlineTooFar`, `AlreadyExists`. |
| `accept_agreement` | `(env, commission_id: Bytes) -> Result<(), AgreementError>` | artist — `record.artist.require_auth()` | `agr_acpt` and `agr_ok` (both topics `(agr_acpt)`/`((agr_ok))`; data `commission_id`) | Requires `Pending`, sets `Active`. Two events are published back-to-back for the same transition. |
| `reject_agreement` | `(env, commission_id: Bytes, reason: String) -> Result<(), AgreementError>` | artist — `record.artist.require_auth()` | `agr_rjct` and `agr_rej` (data `commission_id, reason`) | Validates `reason.len() <= MAX_REASON_LEN` (512) before loading; requires `Pending`, sets `Cancelled`. Two events for one transition. |
| `propose_milestone` | `(env, commission_id: Bytes, milestone_id: Bytes, title: String, amount_usdc: i128) -> Result<(), AgreementError>` | artist — `record.artist.require_auth()` | `ms_prop` (topics `(ms_prop)`, `(ms_new)`; data `commission_id, milestone_id, amount_usdc`) | Requires `Active` and `amount_usdc > 0`; rejects if cumulative proposed milestones would exceed `budget_usdc` (`MilestoneBudgetExceeded`). Writes both `Milestone` and the appended `MilestonesForAgreement`. |
| `approve_milestone` | `(env, commission_id: Bytes, milestone_id: Bytes) -> Result<(), AgreementError>` | client — `record.client.require_auth()` | `ms_appr` and `ms_apprvd` (data `commission_id, milestone_id`) | Takes the `MilestoneLock` (#589) before reading status, releasing it on both the success and `InvalidStatus` paths. Rebuilds the milestone list in place, then flips the agreement to `Completed` if the list is non-empty and every milestone is `Approved`. |
| `get_agreement` | `(env, commission_id: Bytes) -> Result<AgreementRecord, AgreementError>` | none (view) | — | `NotFound` if absent. |
| `get_milestones` | `(env, commission_id: Bytes) -> Result<Vec<MilestoneRecord>, AgreementError>` | none (view) | — | `NotFound` if the `Agreement` key is absent; otherwise an empty `Vec` rather than `NotFound` when the agreement has no milestones. |
| `invite_team_member` | `(env, commission_id: Bytes, member: Address, role: TeamRole, payment_share_bps: u32, contribution_note: String) -> Result<(), AgreementError>` | artist — `record.artist.require_auth()` (the lead artist only) | `tm_invite` (data `commission_id, member, payment_share_bps`) | Requires `Active`. Caps `payment_share_bps` at 10 000, team size at 10 (`TeamSizeLimit`), rejects duplicates (`MemberAlreadyExists`), and enforces a cumulative share ceiling via `checked_add` (`ArithmeticOverflow` / `PaymentShareExceeded`). `role` and `contribution_note` are stored but `role` is never read back for authorization. |
| `accept_team_invitation` | `(env, commission_id: Bytes, member: Address) -> Result<(), AgreementError>` | `member.require_auth()` — the invited address itself | `tm_accept` (data `commission_id, member`) | `NotFound` if `TeamMembers` is unset; `InvalidInvitationStatus` if already terminal; `NotFound` if `member` is not in the list. |
| `decline_team_invitation` | `(env, commission_id: Bytes, member: Address) -> Result<(), AgreementError>` | `member.require_auth()` — the invited address itself | `tm_declin` (data `commission_id, member`) | Mirrors `accept_team_invitation` exactly, setting `InvitationStatus::Declined`. |
| `update_contribution_note` | `(env, commission_id: Bytes, member: Address, note: String) -> Result<(), AgreementError>` | `member.require_auth()` | `tm_note` (data `commission_id, member`) | The doc comment says "the member themselves … or the lead artist", but the code only ever calls `member.require_auth()`. Contains a dead branch at `src/lib.rs:641` — `if env.current_contract_address() != env.current_contract_address()` is always false, so the "auth check – member or lead" block never runs. When `member` is not on the roster the function returns `NotFound` unless `member` equals `record.artist`, in which case it still writes back the unmodified list and publishes `tm_note` — the lead-artist write is therefore a no-op, not an update. |
| `get_team_members` | `(env, commission_id: Bytes) -> Result<Vec<TeamMember>, AgreementError>` | none (view) | — | **This is the function with the unclosed body.** `NotFound` if the `Agreement` key is absent; otherwise an empty `Vec` when the agreement has no team. |
| `set_cancellation_policy` | `(env, commission_id: Bytes, policy: CancellationPolicy) -> Result<(), AgreementError>` | client — `record.client.require_auth()` | `canc_pol` (data `commission_id, policy.penalty_bps, policy.grace_ledgers`) | Client-auth, `Pending` only, so the artist accepts with the exit terms already visible. `cancellation::validate_policy` rejects `penalty_bps > TOTAL_BPS` with `InvalidPolicy`. |
| `get_cancellation_policy` | `(env, commission_id: Bytes) -> CancellationPolicy` | none (view) | — | Falls back to `cancellation::default_policy` — `penalty_bps = DEFAULT_PENALTY_BPS` (1 000), `grace_ledgers = 0` — when unset. Cannot fail. |
| `quote_cancellation` | `(env, commission_id: Bytes, reason: CancellationReason) -> Result<CancellationQuote, AgreementError>` | none (view) | — | Pure preview: loads the record and policy and runs `cancellation::settle` with `Self::in_grace`. Writes nothing. |
| `cancel_agreement` | `(env, commission_id: Bytes, initiator: Address, reason: CancellationReason) -> Result<CancellationRecord, AgreementError>` | `initiator.require_auth()`; then `initiator` must equal `record.client` or `record.artist` or `Unauthorized` | `agr_canc` (data `commission_id, initiator, reason, completion_bps, artist_amount, client_refund`) | Sets `Cancelled`, writes `Cancellation`, and pushes onto the global `CancellationHistory` ring (evicting from the front once it reaches `CANCELLATION_HISTORY_LIMIT` = 50). Rejects a second cancellation (`AlreadyCancelled`) and a `Completed` agreement (`NotCancellable`). Does not move tokens itself. |
| `get_cancellation` | `(env, commission_id: Bytes) -> Result<CancellationRecord, AgreementError>` | none (view) | — | `NotFound` if the agreement was never cancelled. |
| `get_cancellation_history` | `(env) -> Vec<CancellationRecord>` | none (view) | — | Contract-wide, not per-agreement; empty `Vec` when nothing has been cancelled. |
| `set_revision_policy` | `(env, commission_id: Bytes, max_revisions: u32) -> Result<(), AgreementError>` | client — `record.client.require_auth()` | `rev_pol` (data `commission_id, max_revisions`) | Client-auth, `Pending` only, mirroring `set_cancellation_policy`. `revision::validate_policy` rejects `0` or `> MAX_REVISIONS_CAP` (50) with `InvalidPolicy`. |
| `get_revision_policy` | `(env, commission_id: Bytes) -> u32` | none (view) | — | Returns `max_revisions` only; falls back to `DEFAULT_MAX_REVISIONS` (5) when unset. Cannot fail. |
| `request_revision` | `(env, commission_id: Bytes, requester: Address, description: String, deadline_ledger: u32, cost_adjustment: i128) -> Result<u32, AgreementError>` | `requester.require_auth()`; `requester` must be `record.client` or `record.artist` or `Unauthorized` | `rev_new` (data `commission_id, requester, index, cost_adjustment`) | Validates `description.len() <= MAX_REVISION_TEXT_LEN` (512), requires `Active`, and bounds `deadline_ledger` the same way as `create_agreement` (`DeadlineInPast`, `DeadlineTooFar`, `ArithmeticOverflow`). Enforces the policy cap (`RevisionLimitReached`). Returns the new revision's index. `cost_adjustment` is recorded only — the budget is untouched until the other party accepts. |
| `respond_to_revision` | `(env, commission_id: Bytes, responder: Address, revision_index: u32, accept: bool, note: String) -> Result<(), AgreementError>` | `responder.require_auth()`; must be a party (`Unauthorized`); must not be `rev.requester` (`RevisionSameParty`) | `rev_res` (data `commission_id, responder, revision_index, accept`) | Validates `note.len() <= MAX_REVISION_TEXT_LEN` (512). `RevisionNotFound` / `RevisionAlreadyResolved` guards first. On `accept`, `checked_add`s `cost_adjustment` onto `budget_usdc` (`ArithmeticOverflow`) and rejects a non-positive result (`InvalidAmount`); on `reject` the budget is untouched. |
| `get_revisions` | `(env, commission_id: Bytes) -> Vec<RevisionRequest>` | none (view) | — | Whole history; empty `Vec` when none. |
| `get_revision` | `(env, commission_id: Bytes, revision_index: u32) -> Result<RevisionRequest, AgreementError>` | none (view) | — | `RevisionNotFound` for an out-of-range index. |
| `get_revision_count` | `(env, commission_id: Bytes) -> u32` | none (view) | — | `load_revisions(...).len()`. |
| `register_agency` | `(env, agency: Address, name: String, default_split_bps: u32) -> Result<(), AgreementError>` | `agency.require_auth()` | `agy_new` (data `agency, default_split_bps`) | `AgencyExists` on re-registration; `InvalidSplitBps` above `TOTAL_BPS`. Initializes `Agency` and an empty `Roster`. |
| `add_artist` | `(env, agency: Address, artist: Address, split_bps: u32) -> Result<(), AgreementError>` | `agency.require_auth()` | `agy_add` (data `agency, artist, split_bps`) | `AgencyNotFound` if unregistered, `InvalidSplitBps` if out of range, `ArtistAlreadyRepresented` if the artist already has an `ArtistAgency`. Writes `RosterEntry` + `ArtistAgency`, appends to `Roster`, bumps `profile.artist_count` and syncs it into `AgencyAnalytics`. |
| `remove_artist` | `(env, agency: Address, artist: Address) -> Result<(), AgreementError>` | `agency.require_auth()` | `agy_rm` (data `agency, artist`) | `AgencyNotFound` / `ArtistNotOnRoster` guards. Removes `ArtistAgency`, filters `Roster`, recomputes `artist_count`, and syncs analytics. The `RosterEntry` and its historic `gross_distributed` / `agency_revenue` / `artist_payouts` totals are deliberately left in storage for auditability — so a removed artist can be re-added to a different agency, and the stale entry remains readable via `get_roster_entry`. |
| `set_artist_split` | `(env, agency: Address, artist: Address, split_bps: u32) -> Result<(), AgreementError>` | `agency.require_auth()` | `agy_split` (data `agency, artist, split_bps`) | Renegotiates the rate on an existing roster entry. `AgencyNotFound` / `InvalidSplitBps` / `ArtistNotOnRoster`. |
| `distribute_batch` | `(env, agency: Address, token_address: Address, payments: Vec<BatchPayment>) -> Result<i128, AgreementError>` | `agency.require_auth()` | `agy_batch` (data `agency, payments.len(), total_gross`) | `EmptyBatch` on an empty vec; `InvalidAmount` above `agency::MAX_BATCH` (25). Commits every roster-entry update and the analytics roll-up **before** any `token_client.transfer(&agency, &payment.artist, &net)`, so a failure on any line reverts the whole batch. Uses `token::Client::new(&env, &token_address)`; the token address is caller-supplied and unvalidated. Returns the total gross distributed. |
| `get_agency` | `(env, agency: Address) -> Result<AgencyProfile, AgreementError>` | none (view) | — | `AgencyNotFound` if unregistered. |
| `get_roster` | `(env, agency: Address) -> Vec<Address>` | none (view) | — | Empty `Vec` for an unknown agency rather than an error. |
| `get_roster_entry` | `(env, agency: Address, artist: Address) -> Result<RosterEntry, AgreementError>` | none (view) | — | `ArtistNotOnRoster` if the pair is absent. |
| `get_artist_agency` | `(env, artist: Address) -> Option<Address>` | none (view) | — | Returns the `Option` straight from storage; `None` for independent artists. |
| `get_agency_analytics` | `(env, agency: Address) -> AgencyAnalytics` | none (view) | — | `unwrap_or_default()`; an all-zero `AgencyAnalytics` for an unknown agency. |
| `health_check` | `(env) -> shared::health::HealthReport` | none (view) | `hlth_alrt` (published by `shared` only when `report.anomaly`); may also trigger `rollback` via `maybe_auto_rollback` | Wraps `shared::health::health_check`, and on an anomalous report calls `shared::rollout::maybe_auto_rollback`. |
| `get_health_metrics` | `(env) -> shared::health::HealthMetrics` | none (view) | — | Straight delegation to `shared::health::get_metrics`. |
| `get_sla_targets` | `(env) -> shared::health::SlaTargets` | none (view) | — | Binds `env` to `_` and returns the compile-time `shared::health::sla_targets()`. |
| `set_alert_config` | `(env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `alrt_cfg` (data `config.unhealthy_error_bps`; published by `shared`) | Note the crate has no stored admin key: authorization is purely "whatever address the caller passes as `admin` must sign", with no check that it is a privileged role. |
| `get_alert_config` | `(env) -> shared::health::AlertConfig` | none (view) | — | Straight delegation to `shared::health::get_alert_config`. |
| `detect_anomaly` | `(env) -> bool` | none (view) | `hlth_alrt` (conditionally, published by `shared`) | Straight delegation to `shared::health::detect_anomaly`. |
| `report_ok` | `(env, admin: Address)` | `admin.require_auth()` | — | Records a success on the shared metrics. No contract-level admin check (see `set_alert_config`). |
| `report_error` | `(env, admin: Address)` | `admin.require_auth()` | — | Records a failure on the shared metrics. No contract-level admin check. |
| `set_feature_flag` | `(env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool)` | `admin.require_auth()` | `feat_flg` (data `flag, enabled`; published by `shared`) | Delegates to `shared::rollout::set_feature_flag`. |
| `is_feature_enabled` | `(env, flag: soroban_sdk::Symbol) -> bool` | none (view) | — | Delegates to `shared::rollout::is_feature_enabled`. |
| `set_canary_deployment` | `(env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `canary` (data `canary, stable, canary_bps`; published by `shared`) | Delegates to `shared::rollout::set_canary_deployment`. |
| `route_to_canary` | `(env, caller: Address) -> bool` | none (view) | — | Reads only; the `caller` argument is not authenticated. Delegates to `shared::rollout::route_to_canary`. |
| `get_rollout_state` | `(env) -> shared::rollout::RolloutState` | none (view) | — | Delegates to `shared::rollout::get_state`. |
| `set_rollback_trigger` | `(env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `rb_trig` (data `error_bps`; published by `shared`) | Delegates to `shared::rollout::set_rollback_trigger`. |
| `should_rollback` | `(env) -> bool` | none (view) | `rollback` (conditionally, published by `shared`) | Delegates to `shared::rollout::should_rollback`. |
| `trigger_rollback` | `(env, admin: Address)` | `admin.require_auth()` | `rollback` (data = current ledger sequence; published by `shared`) | Delegates to `shared::rollout::trigger_rollback`. |

Total: **54** externally callable functions — 51 written literally in `lib.rs` plus 3 generated by `impl_semver_queries!()`. The private `fn in_grace` in the plain `impl` block at `src/lib.rs:1327`–`1333`, and the free helpers at `src/lib.rs:61`–`161`, are internal and excluded.

**Events.**

22 `env.events().publish(...)` call sites in `src/lib.rs`, all distinct in shape — no two are identical. Twelve use the single-argument form (one topic, data inline); three use the three-argument form (topic tuple, a second "kind" argument, then the data tuple); the rest are as listed. No `env.events()` call appears in `types.rs`, `errors.rs`, `agency.rs`, `cancellation.rs` or `revision.rs`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `agr_crtd` | `((symbol_short!("agr_crtd"),), (symbol_short!("agr_new"),))` | `(commission_id, client, artist, budget_usdc)` | `create_agreement` |
| `agr_acpt` | `((symbol_short!("agr_acpt"),),)` | `(commission_id,)` | `accept_agreement` |
| `agr_ok` | `((symbol_short!("agr_ok"),),)` | `(commission_id,)` | `accept_agreement` (emitted immediately after `agr_acpt`) |
| `agr_rjct` | `((symbol_short!("agr_rjct"),),)` | `(commission_id, reason)` | `reject_agreement` |
| `agr_rej` | `((symbol_short!("agr_rej"),),)` | `(commission_id, reason)` | `reject_agreement` (emitted immediately after `agr_rjct`) |
| `ms_prop` | `((symbol_short!("ms_prop"),), (symbol_short!("ms_new"),))` | `(commission_id, milestone_id, amount_usdc)` | `propose_milestone` |
| `ms_appr` | `((symbol_short!("ms_appr"),),)` | `(commission_id, milestone_id)` | `approve_milestone`, **before** the lock is released |
| `ms_apprvd` | `((symbol_short!("ms_apprvd"),),)` | `(commission_id, milestone_id)` | `approve_milestone`, **after** the lock is released |
| `tm_invite` | `((symbol_short!("tm_invite"),),)` | `(commission_id, member, payment_share_bps)` | `invite_team_member` |
| `tm_accept` | `((symbol_short!("tm_accept"),),)` | `(commission_id, member)` | `accept_team_invitation` |
| `tm_declin` | `((symbol_short!("tm_declin"),),)` | `(commission_id, member)` | `decline_team_invitation` |
| `tm_note` | `((symbol_short!("tm_note"),),)` | `(commission_id, member)` | `update_contribution_note` (does **not** carry the new `note`) |
| `canc_pol` | `((symbol_short!("canc_pol"),),)` | `(commission_id, policy.penalty_bps, policy.grace_ledgers)` | `set_cancellation_policy` |
| `agr_canc` | `((symbol_short!("agr_canc"),),)` | `(commission_id, initiator, reason, completion_bps, artist_amount, client_refund)` | `cancel_agreement` — does not carry `penalty` or `penalised`; asserted on in `cancellation_tests.rs:250` |
| `rev_pol` | `((symbol_short!("rev_pol"),),)` | `(commission_id, max_revisions)` | `set_revision_policy` |
| `rev_new` | `((symbol_short!("rev_new"),),)` | `(commission_id, requester, index, cost_adjustment)` | `request_revision` |
| `rev_res` | `((symbol_short!("rev_res"),),)` | `(commission_id, responder, revision_index, accept)` | `respond_to_revision` |
| `agy_new` | `((symbol_short!("agy_new"),),)` | `(agency, default_split_bps)` | `register_agency` |
| `agy_add` | `((symbol_short!("agy_add"),),)` | `(agency, artist, split_bps)` | `add_artist` |
| `agy_rm` | `((symbol_short!("agy_rm"),),)` | `(agency, artist)` | `remove_artist` |
| `agy_split` | `((symbol_short!("agy_split"),),)` | `(agency, artist, split_bps)` | `set_artist_split` |
| `agy_batch` | `((symbol_short!("agy_batch"),),)` | `(agency, payments.len(), total_gross)` | `distribute_batch` — carries the gross total, not the agency cut |
| `alrt_cfg` | `((symbol_short!("alrt_cfg"),),)` | `config.unhealthy_error_bps` | `set_alert_config` — published by `shared::health::set_alert_config` |
| `hlth_alrt` | `((symbol_short!("hlth_alrt"),),)` | unknown — payload not read; declared in `contracts/shared/src/health.rs:251` | `shared::health::detect_anomaly`, reached from this contract's `detect_anomaly` and `health_check` |
| `feat_flg` | `((symbol_short!("feat_flg"),),)` | `(flag, enabled)` | `set_feature_flag` — published by `shared::rollout` |
| `canary` | `((symbol_short!("canary"),),)` | `(canary, stable, canary_bps)` | `set_canary_deployment` — published by `shared::rollout` |
| `rb_trig` | `((symbol_short!("rb_trig"),),)` | `error_bps` | `set_rollback_trigger` — published by `shared::rollout` |
| `rollback` | `((symbol_short!("rollback"),),)` | `env.ledger().sequence()` | `trigger_rollback`, and conditionally `should_rollback` / `maybe_auto_rollback` — published by `shared::rollout` |

**Errors.**

The crate's single `#[contracterror]` enum is `AgreementError` (`src/errors.rs:3-46`), `#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)] #[repr(u32)]`. It has **30 variants but only 26 distinct discriminants**: `InputTooLong` reuses 10, `DeadlineTooFar` reuses 11, `MilestoneLocked` reuses 12, and `NotCancellable` reuses 13. Rust permits this (explicit discriminants need not be unique), so it is not a compile error, but the collisions make the on-wire error code ambiguous — e.g. `InputTooLong` and `MemberAlreadyExists` are indistinguishable to a client decoding the code. The `Code` column below gives the literal `#[repr(u32)]` value as written.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyExists` | 1 | An `Agreement` record already exists for `commission_id` (`create_agreement`). |
| `NotFound` | 2 | No record at the requested key: agreement, milestone, team-member list, cancellation, revision index, team member, or the `member` absent from an otherwise-present team list. |
| `InvalidStatus` | 3 | The agreement's `AgreementStatus` does not permit the transition — e.g. accept/reject require `Pending`, milestones and invitations and revisions require `Active`, policy setters require `Pending`. |
| `Unauthorized` | 4 | `initiator` (`cancel_agreement`), `requester` (`request_revision`) or `responder` (`respond_to_revision`) is neither the client nor the artist. |
| `InvalidAmount` | 5 | `budget_usdc <= 0` or `amount_usdc <= 0`; `payment_share_bps > 10_000`; a `distribute_batch` longer than `MAX_BATCH` (25); a non-positive `gross_usdc`; an accepted revision that would drive `budget_usdc` to `<= 0`. |
| `DeadlineInPast` | 6 | `deadline_ledger <= env.ledger().sequence()` in `create_agreement` or `request_revision`. |
| `MilestoneBudgetExceeded` | 7 | Cumulative proposed milestone amounts would exceed `budget_usdc` (`propose_milestone`). |
| `NotAllMilestonesApproved` | 8 | **Never constructed anywhere in the crate** (`lib.rs` has 0 uses) — declared and given a `Display` arm and a `NOT_ALL` suggestion, but dead. Its intended guard is not present in the current source. |
| `ArithmeticOverflow` | 9 | Any `checked_add`/`checked_mul` failure: the `now + MAX_DEADLINE_OFFSET_LEDGERS` bound, the cumulative team-share sum, the revision budget adjustment, `split_payment`'s multiply, and `settle`'s arithmetic. |
| `MemberAlreadyExists` | 10 | `invite_team_member` names an address already on the team. |
| `PaymentShareExceeded` | 11 | Cumulative `payment_share_bps` would exceed 10 000 after the addition. |
| `InvalidInvitationStatus` | 12 | `accept_team_invitation` / `decline_team_invitation` on an invitation already `Accepted` or `Declined`. |
| `TeamSizeLimit` | 13 | `invite_team_member` on an agreement that already has 10 members. |
| `InputTooLong` | 10 (collides with `MemberAlreadyExists`) | `commission_id.len() > MAX_ID_LEN` (64), `title.len() > MAX_TITLE_LEN` (128), `milestone_id.len() > MAX_ID_LEN`, milestone `title.len() > MAX_MILESTONE_TITLE_LEN` (128), `reason.len() > MAX_REASON_LEN` (512), `description.len() > MAX_REVISION_TEXT_LEN` (512), or `note.len() > MAX_REVISION_TEXT_LEN` (closes #591). |
| `DeadlineTooFar` | 11 (collides with `PaymentShareExceeded`) | `deadline_ledger > current_sequence + MAX_DEADLINE_OFFSET_LEDGERS` in `create_agreement` or `request_revision` (closes #592). |
| `MilestoneLocked` | 12 (collides with `InvalidInvitationStatus`) | `DataKey::MilestoneLock` is already set for this `(commission_id, milestone_id)` — a concurrent transition is in flight; caller should retry (closes #589). |
| `NotCancellable` | 13 (collides with `TeamSizeLimit`) | `cancel_agreement` on an agreement that is neither `Pending` nor `Active` (i.e. `Completed` or `Cancelled` reached some other way). |
| `AlreadyCancelled` | 14 | `cancel_agreement` on an agreement already in `AgreementStatus::Cancelled`. |
| `InvalidPolicy` | 15 | `cancellation::validate_policy`: `penalty_bps > TOTAL_BPS` (10 000). `revision::validate_policy`: `max_revisions == 0` or `> MAX_REVISIONS_CAP` (50). Raised from `src/cancellation.rs:101` and `src/revision.rs:67`. |
| `AgencyExists` | 16 | `register_agency` for an address that already has an `AgencyProfile`. |
| `AgencyNotFound` | 17 | `load_agency` finds no `Agency` record for the address. |
| `ArtistNotOnRoster` | 18 | `load_roster_entry` finds no `RosterEntry` for the `(agency, artist)` pair. |
| `ArtistAlreadyRepresented` | 19 | `add_artist` for an artist who already has a `DataKey::ArtistAgency` entry. |
| `InvalidSplitBps` | 20 | `agency::validate_split_bps`: `split_bps > TOTAL_BPS` (10 000). Raised from `src/agency.rs:63` on `register_agency`, `add_artist` and `set_artist_split`. |
| `EmptyBatch` | 21 | `distribute_batch` with an empty `payments` vec. |
| `RevisionNotFound` | 22 | `revision_index` is out of range for the agreement's revision list. |
| `RevisionLimitReached` | 23 | `request_revision` when `revisions.len() >= policy.max_revisions`. |
| `RevisionAlreadyResolved` | 24 | `respond_to_revision` on a revision whose status is not `Pending`. |
| `RevisionSameParty` | 25 | `responder == rev.requester` — the requesting party cannot resolve their own revision. |
| `RateLimiterNotConfigured` | 26 | `create_agreement` finds no `DataKey::RateLimiter` instance entry. **Unreachable today** — the variant is referenced at `src/lib.rs:216`, but `DataKey::RateLimiter` is not a declared variant, so this code does not compile. |

`errors.rs` also provides `get_suggestion(error: AgreementError) -> Symbol` (marked `#[allow(dead_code)]`, so nothing calls it in-crate) mapping each variant to a short code — `DUP`, `NOT_FOUND`, `BAD_STS`, `AUTH`, `BAD_AMT`, `PAST_DDL`, `OVER_BUD`, `NOT_ALL`, `OVERFL`, `DUP_MBR`, `SHRE_LIM`, `BAD_INV`, `TEAM_LIM`, `TOO_LONG`, `FAR_DDL`, `MS_LOCK`, `NO_CANCL`, `CANCELLED`, `BAD_POL`, `AGY_DUP`, `NO_AGY`, `NO_ROSTER`, `REPPED`, `BAD_BPS`, `NO_BATCH`, `NO_REV`, `REV_MAX`, `REV_DONE`, `REV_SELF`, `NO_RL`.

**Storage.** See [STORAGE.md](./STORAGE.md#commission-agreement).

**Compile status.** Does not compile. `cargo build -p commission_agreement` fails with `error: this file contains an unclosed delimiter`, anchored at `src/lib.rs:1335:23` (the `mod integration_tests;` token), with the compiler pointing at the unclosed `{` of the `#[contractimpl] impl CommissionAgreementContract` block at `src/lib.rs:186` and identifying the `{` that opens `get_team_members`' body at `src/lib.rs:678` as the delimiter "that might not be properly closed"; the `}` at `src/lib.rs:1324` is the candidate it matches, and it is consumed as that function's missing brace. Net effect: everything from `src/lib.rs:685` to `src/lib.rs:1323` — 41 `pub fn` entry points covering cancellation, revisions, the agency layer, and all `shared` health/rollout wrappers — is parsed as statements inside `get_team_members`' body rather than as sibling contract functions, and the `#[contractimpl]` block is never closed. Additionally, three latent errors are masked behind the parse failure and would surface next: the duplicate `use types::{…}` at `src/lib.rs:37-38` and `src/lib.rs:54`; the reference to the undeclared `DataKey::RateLimiter` variant at `src/lib.rs:215`; and the reference to the non-existent `types::RateLimitKey::CommissionsPerArtist` at `src/lib.rs:222`. Separately, `src/milestone_flow.rs`, `src/multiple_escrows.rs`, `src/dispute_resolution.rs` and `src/test.rs` are all gated behind the non-default `legacy_tests` feature and are independently broken (they import `crate::test::{helpers, test_lifecycles}` and `crate::types::{Client, Commission, Milestone}`, none of which exist), so re-enabling that feature fails too. `src/integration_tests.rs` is never compiled because the `mod` statement that would pull it in sits outside the unclosed block. The tables above document what is *written* in the source; because of the parse error, no public function past `get_team_members` is actually reachable as a contract entry point, and the code from `src/lib.rs:685` onward is documentation of intent rather than of working behaviour. Not fixed, as instructed.


### competitions

> unknown — `contracts/competitions/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc; `src/types.rs` and `src/errors.rs` have none either. The summary below is reconstructed from the source (contracts/competitions, closes no numbered issue; `MAX_PRIZE_POSITIONS`/`TOTAL_BPS` and the rank-and-payout flow are the contract's substance).

Source: `contracts/competitions/`

**Purpose.**

`competitions` runs prize competitions (jams, showcases, contests) end to end on-chain: an organizer escrows a prize pool into the contract, entrants submit entries during a fixed window, reputation-weighted voters ballot in a second window, and a finalization step ranks the entries and pays out by a fixed bps split. It is built for organizers who want the escrow, the vote tally, the ranking, and the payout to be one atomic, auditable pipeline rather than an off-chain spreadsheet plus manual transfers. Reputation is the anti-sybil lever: voters carry an admin-attested score, their ballot weight equals that score, and accounts below the competition's `min_reputation` cannot vote at all. Unfilled prize positions and rounding dust are both handled explicitly — dust is folded into the top prize when every position is filled, and anything genuinely unawarded goes back to the organizer. A capped `History` deque records one summary line per finalized competition for reporting. The crate also carries the shared health (#678) and gradual-rollout (#684) surface.

**Dependencies.**

Path dep: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — used only for `shared::health` and `shared::rollout`; the semver surface is *not* taken from it, because `lib.rs` does `include!("../../semver_types.rs")` instead. Registry dep: `soroban-sdk` `21.0.0` (workspace-inherited; dev-dependency adds `testutils`; a pass-through `testutils` feature is declared). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `[lib] crate-type = ["cdylib", "rlib"]`.

**Public interface.**

32 externally callable `pub fn`s in the single `#[contractimpl] impl Competitions` block: 29 written literally in the source plus three produced by the `impl_semver_queries!()` macro expansion (the `semver_types.rs` variant, which reads `CARGO_PKG_VERSION` rather than storage). The free `fn`s in the module — `has_admin`, `require_admin`, `require_initialized`, `load_competition`, `save_competition`, `load_submission`, `entrants_of`, `reputation_of`, `validate_rules`, `rank_entries` — are internal and omitted.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address, history_limit: u32) -> Result<(), CompetitionError>` | **none** — no `require_auth` call | `init` | First-call-wins: returns `AlreadyInitialized` if `DataKey::Admin` already exists, and `InvalidRules` if `history_limit == 0`. Any address can initialize an uninitialized instance, so there is a capture window on deployment |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | From the `semver_types.rs` macro; parses `CARGO_PKG_VERSION` and reads no storage |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Name, semver, min-compatible client, `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Major must match; on `0.x` the minor must match too |
| `set_reputation` | `set_reputation(env: Env, account: Address, score: u32) -> Result<(), CompetitionError>` | stored `admin.require_auth()` via `require_admin` (returns `NotInitialized` if no admin) | `rep_set` | The only genuinely admin-gated function in the core competition flow. Reputation is computed off-chain and attested here; it is never derived on-chain |
| `get_reputation` | `get_reputation(env: Env, account: Address) -> u32` | none (view) | — | `0` for an unknown account |
| `create_competition` | `create_competition(env: Env, id: Bytes, organizer: Address, token_address: Address, title: String, prize_pool: i128, rules: CompetitionRules) -> Result<(), CompetitionError>` | `organizer.require_auth()` | `comp_new` | Escrows the pool: `token::Client::transfer(organizer → contract, prize_pool)`. Rejects a duplicate `id` (`CompetitionExists`), a non-positive pool (`InvalidPrizePool`), and rules where any window/cap is `0`, the split is empty or longer than 10 positions, any share is `0`, the shares overflow while summing, or the shares do not total exactly 10,000 bps (`InvalidRules` / `ArithmeticOverflow`). Computes `submission_end_ledger` and `voting_end_ledger` from `env.ledger().sequence()` at creation; rules are thereafter immutable |
| `submit` | `submit(env: Env, competition_id: Bytes, entrant: Address, entry_uri: String) -> Result<(), CompetitionError>` | `entrant.require_auth()` | `submit` | One entry per entrant, capped by `rules.max_submissions`. Returns `SubmissionsClosed` if the status is not `Open` or `sequence > submission_end_ledger`, `AlreadySubmitted` on a duplicate, `TooManySubmissions` at the cap. Appends to `Entrants` and bumps `submission_count` |
| `vote` | `vote(env: Env, competition_id: Bytes, voter: Address, entrant: Address) -> Result<u32, CompetitionError>` | `voter.require_auth()` | `vote` | Returns the weight applied. Window is enforced at both ends: `VotingNotOpen` while `sequence <= submission_end_ledger`, `VotingClosed` after `voting_end_ledger` or once status leaves `Open`. `SelfVoteNotAllowed` if `voter == entrant`; `ReputationTooLow` if weight is `0` or below `rules.min_reputation`; `AlreadyVoted` if `Voted(competition, voter)` exists. Records the choice, adds weight to `submission.votes` and `competition.total_votes` |
| `finalize` | `finalize(env: Env, competition_id: Bytes) -> Result<Vec<Winner>, CompetitionError>` | **none — permissionless** | `final` | Returns the fixed ranking. `AlreadyFinalized` unless status is `Open`; `VotingClosed` while `sequence <= voting_end_ledger` (the "cannot be held open by an absent organizer" path). Ranks by weighted votes with ties going to the earlier submission; pays out `prize_pool * share / 10_000` per position; folds rounding dust into rank 1 only when every position was filled; stores `Winners` + `History` and sets status `Finalized`. `ArithmeticOverflow` on a `checked_mul` failure |
| `distribute_prizes` | `distribute_prizes(env: Env, competition_id: Bytes) -> Result<(), CompetitionError>` | **none — permissionless** | `prizes` | `NotFinalized` while `Open`, `AlreadySettled` once `Settled`. Sets `Settled` *before* transferring, pays each winner from contract balance, and returns the unawarded remainder to the organizer. Reads `Winners` with an empty-vector default, so a `Finalized` competition with no stored winners refunds the whole pool |
| `get_competition` | `get_competition(env: Env, competition_id: Bytes) -> Result<Competition, CompetitionError>` | none (view) | — | `CompetitionNotFound` for an unknown id |
| `get_submission` | `get_submission(env: Env, competition_id: Bytes, entrant: Address) -> Result<Submission, CompetitionError>` | none (view) | — | `SubmissionNotFound` when the entrant never submitted |
| `get_entrants` | `get_entrants(env: Env, competition_id: Bytes) -> Vec<Address>` | none (view) | — | Empty vec for an unknown competition; also the deterministic tie-break order |
| `get_winners` | `get_winners(env: Env, competition_id: Bytes) -> Vec<Winner>` | none (view) | — | Empty vec before `finalize` |
| `get_history` | `get_history(env: Env) -> Vec<CompetitionSummary>` | none (view) | — | Oldest-first, capped at `HistoryLimit` |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none | `hlth_alrt`, `rollback` | **Not a pure view**: writes `LastAlertLedger` and may auto-rollback the rollout state. No auth |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | — |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Compile-time constants; `env` unused |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` — **caller-supplied, never compared to `DataKey::Admin`** | `alrt_cfg` | Unlike `set_reputation`, the admin-gated health path does not call `require_admin`, so any address can rewrite thresholds by signing for itself |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to the shared default |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | — |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | Counter sample supplied by an operator; no core entry point reports automatically |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | — |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `feat_flg` | Appends to `FlagIndex` on first set |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | `false` once `RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `canary` | Aborts `"canary_bps exceeds 10000"` |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | none | — | Sticky `SHA-256(caller XDR) mod 10000 < canary_bps`; `caller` is unauthenticated, so the result is advisory |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | — |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rb_trig` | Aborts `"invalid rollback trigger"` for `0` or `> 10000` |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | — |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rollback`, `contract_paused` | Also sets `shared::pause::PauseDataKey::Paused = true`, which this contract has no other way to clear |

**Events.**

Seven contract-specific shapes (all `symbol_short!`, so ≤9 characters) plus seven delegated from `shared`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `(admin: Address, history_limit: u32)` | `initialize` |
| `rep_set` | `(symbol_short!("rep_set"),)` | `(account: Address, score: u32)` | `set_reputation` |
| `comp_new` | `(symbol_short!("comp_new"),)` | `(id: Bytes, organizer: Address, prize_pool: i128)` | `create_competition` — the escrow `token::transfer` is not represented as an event |
| `submit` | `(symbol_short!("submit"),)` | `(competition_id: Bytes, entrant: Address)` | `submit` — `entry_uri` is not included |
| `vote` | `(symbol_short!("vote"),)` | `(competition_id: Bytes, voter: Address, entrant: Address, weight: u32)` | `vote` |
| `final` | `(symbol_short!("final"),)` | `(competition_id: Bytes, winners.len(): u32)` — the count, not the ranking | `finalize` |
| `prizes` | `(symbol_short!("prizes"),)` | `(competition_id: Bytes, paid: i128, unawarded: i128)` | `distribute_prizes` — individual winner amounts are not in the event |
| `contract_paused` | `(Symbol::new(env, "contract_paused"),)` | `shared::pause::ContractPausedEvent { admin: Address }` | `trigger_rollback` only |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check`, cooldown-gated |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `config.unhealthy_error_bps: u32` | `set_alert_config` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `error_bps: u32` | `set_rollback_trigger` |
| `rollback` | `(symbol_short!("rollback"),)` | `env.ledger().sequence(): u32` | `trigger_rollback`, and `health_check` via `maybe_auto_rollback` |

**Errors.**

`errors::CompetitionError`, `#[contracterror]` with `#[repr(u32)]` and explicit `= N` discriminants on every unit variant. The file also carries a `Display` impl and a `get_suggestion` mapping, the same house pattern as the other new-style crates.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` found `DataKey::Admin` already present in instance storage |
| `NotInitialized` | 2 | `require_admin` found no `DataKey::Admin` — from `set_reputation`; also the value `require_initialized` would return, though that helper's `Err` is only ever reachable pre-`initialize` |
| `Unauthorized` | 3 | **Never constructed.** Defined and mapped to the `AUTH` suggestion, but no code path in the crate returns it — `require_admin` delegates authorization to `admin.require_auth()` instead |
| `CompetitionNotFound` | 4 | No `DataKey::Competition(id)` entry — from `load_competition`, i.e. `vote`, `finalize`, `distribute_prizes`, `get_competition` |
| `CompetitionExists` | 5 | `create_competition` found `DataKey::Competition(id)` already present |
| `InvalidRules` | 6 | `create_competition` with `submission_ledgers == 0`, `voting_ledgers == 0`, `max_submissions == 0`, an empty `prize_split_bps`, more than `MAX_PRIZE_POSITIONS` (10) positions, any zero share, or shares that do not total `TOTAL_BPS` (10,000); also `initialize` with `history_limit == 0` |
| `InvalidPrizePool` | 7 | `create_competition` with `prize_pool <= 0` |
| `SubmissionsClosed` | 8 | `submit` when the status is not `Open` or `env.ledger().sequence() > submission_end_ledger` |
| `VotingNotOpen` | 9 | `vote` while `sequence <= submission_end_ledger` |
| `VotingClosed` | 10 | `vote` after `voting_end_ledger` or with a non-`Open` status; also `finalize` before the voting window closes |
| `AlreadySubmitted` | 11 | `submit` where `DataKey::Submission(competition_id, entrant)` already exists |
| `SubmissionNotFound` | 12 | `load_submission` found no entry — from `vote` (voting for a non-entrant) and `get_submission` |
| `TooManySubmissions` | 13 | `submit` when `competition.submission_count >= rules.max_submissions` |
| `AlreadyVoted` | 14 | `vote` where `DataKey::Voted(competition_id, voter)` already exists |
| `SelfVoteNotAllowed` | 15 | `vote` with `voter == entrant` |
| `ReputationTooLow` | 16 | `vote` where the voter's weight is `0` or below `rules.min_reputation` |
| `NotFinalized` | 17 | `distribute_prizes` while the competition is still `Open` |
| `AlreadyFinalized` | 18 | `finalize` on a competition whose status is not `Open` (i.e. `Finalized` or `Settled`) |
| `AlreadySettled` | 19 | `distribute_prizes` on a competition already in `Settled` |
| `ArithmeticOverflow` | 20 | `checked_add` while summing `prize_split_bps`, or `checked_mul` on `prize_pool * share` during prize computation |

**Storage.** See [STORAGE.md](./STORAGE.md#competitions).

**Compile status.** `compiles` — `cargo check -p competitions` and `cargo test -p competitions --no-run` both succeed with no warnings. The 22 unit tests in `src/test.rs` compile against the `rlib` target.


### creator_fund

> unknown — `contracts/creator_fund/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc; `src/types.rs` and `src/errors.rs` have none either. The summary below is reconstructed from the source (contributor-weighted capital pools with steward-configured distribution rules, closes no numbered issue).

Source: `contracts/creator_fund/`

**Purpose.**

`creator_fund` is a contributor-governed capital pool for creator programmes — grant pools, emergency relief, platform initiatives, matching pools. Contributors deposit tokens and those deposits are the *only* source of voting power, so governance is capital-weighted and sybil costs scale with influence. A steward configures the fund's guardrails (per-payout cap in bps, minimum reserve, quorum share, voting window) and opens funds, but cannot spend from them: any payout must be proposed by a contributor, approved by a capital-weighted vote that reaches quorum, and only then executed. The rule is re-checked at execution time against the balance *then*, so a steward who tightens the reserve after approval can block an already-voted payout. The contract also keeps two capped append-only logs per fund — a growth series for charting and an allocations trail for audit — so a fund's whole life is readable without replaying the event log. It also carries the shared health (#678) and gradual-rollout (#684) surface.

**Dependencies.**

Path dep: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — used only for `shared::health` and `shared::rollout`; as with `competitions`, the semver surface comes from `include!("../../semver_types.rs")` rather than from `shared::version`, so there is no `UpgradeKey::Version` in this contract's storage. Registry dep: `soroban-sdk` `21.0.0` (workspace-inherited; dev-dependency adds `testutils`; a pass-through `testutils` feature is declared). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `[lib] crate-type = ["cdylib", "rlib"]`.

**Public interface.**

32 externally callable `pub fn`s in the single `#[contractimpl] impl CreatorFund` block: 29 written literally in the source plus three produced by the `impl_semver_queries!()` expansion in `semver_types.rs`. The module-level `fn`s — `has_admin`, `require_initialized`, `load_fund`, `save_fund`, `load_proposal`, `save_proposal`, `validate_rule`, `history_limit`, `record_growth`, `voting_power`, `check_distribution_rule` — are internal and omitted.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address, history_limit: u32) -> Result<(), FundError>` | **none** — no `require_auth` call | `init` | Returns `AlreadyInitialized` if `DataKey::Admin` is already set, `InvalidAmount` if `history_limit == 0`. Deployment-capture window: whoever calls first sets the admin, and the admin then has no powers here anyway |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | From `semver_types.rs`; parses `CARGO_PKG_VERSION`, no storage read |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Name, semver, min-compatible client, `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Major must match; on `0.x` the minor must match |
| `create_fund` | `create_fund(env: Env, fund_id: Bytes, fund_type: FundType, steward: Address, token: Address, rule: DistributionRule) -> Result<(), FundError>` | `steward.require_auth()` | `fund_new` | `FundExists` on a duplicate `fund_id`; `InvalidRule` when `max_allocation_bps == 0` or `> 10_000`, `quorum_bps > 10_000`, `voting_ledgers == 0`, or `min_reserve < 0`. Starts at zero balance and immediately writes the first `Growth` snapshot |
| `set_rule` | `set_rule(env: Env, fund_id: Bytes, rule: DistributionRule) -> Result<(), FundError>` | `fund.steward.require_auth()` — the steward read from storage, not a parameter | `rule` | Replaces the whole `DistributionRule` after re-validating it. Because the cap and reserve are re-checked at execution time, this is the steward's lever to block a payout that was already approved (see the `rule_is_rechecked_at_execution_time` test) |
| `contribute` | `contribute(env: Env, fund_id: Bytes, contributor: Address, amount: i128) -> Result<(), FundError>` | `contributor.require_auth()` | `contrib` | Accumulates into `Contribution(fund_id, contributor)` (so repeat deposits add voting power) and increments `contributor_count` only on the account's first deposit. Then pulls tokens `contributor → contract` and appends a growth snapshot. `InvalidAmount` for `amount <= 0`; `ArithmeticOverflow` on the balance/total adds |
| `propose_allocation` | `propose_allocation(env: Env, proposal_id: Bytes, fund_id: Bytes, proposer: Address, recipient: Address, amount: i128, memo: String) -> Result<(), FundError>` | `proposer.require_auth()` | `proposed` | `ProposalExists` on a duplicate id; `NotContributor` when the proposer's voting power is `0`; `InvalidAmount` for `amount <= 0`; and the distribution rule is checked up front via `check_distribution_rule` (`ExceedsAllocationLimit` when `amount > balance * max_allocation_bps / 10_000`, `ReserveBreached` when `balance - amount < min_reserve`). `voting_ends_ledger` is fixed from the current ledger plus `rule.voting_ledgers`. `memo` has no length limit |
| `vote` | `vote(env: Env, proposal_id: Bytes, voter: Address, support: bool) -> Result<i128, FundError>` | `voter.require_auth()` | `vote` | Returns the weight applied. `VotingClosed` unless status is `Voting` and `sequence <= voting_ends_ledger`; `NotContributor` when voting power is `0`; `AlreadyVoted` when `Voted(proposal_id, voter)` exists. Weight is the contributor's own contributed capital, added to `votes_for` or `votes_against` |
| `finalize_proposal` | `finalize_proposal(env: Env, proposal_id: Bytes) -> Result<ProposalStatus, FundError>` | **none — permissionless** | `finalize` | Returns the resulting status. `VotingClosed` unless status is `Voting`; `VotingOpen` while `sequence <= voting_ends_ledger`. Quorum is capital turnout (`votes_for + votes_against >= total_contributed * quorum_bps / 10_000`) and the proposal must also win strictly on `votes_for > votes_against`; otherwise `Rejected`. The `QuorumNotMet` variant exists but this logic reports `Rejected` instead, so it is never returned |
| `execute_allocation` | `execute_allocation(env: Env, proposal_id: Bytes) -> Result<(), FundError>` | **none — permissionless** | `executed` | `ProposalNotApproved` unless status is `Approved`. Re-runs `check_distribution_rule` against the *current* balance, debits `fund.balance`, bumps `total_allocated`, marks the proposal `Executed`, appends to `Allocations`, records a growth snapshot, then transfers `contract → recipient`. `ArithmeticOverflow` on the `total_allocated` add |
| `get_fund` | `get_fund(env: Env, fund_id: Bytes) -> Result<Fund, FundError>` | none (view) | — | `FundNotFound` for an unknown id |
| `get_proposal` | `get_proposal(env: Env, proposal_id: Bytes) -> Result<Proposal, FundError>` | none (view) | — | `ProposalNotFound` for an unknown id |
| `get_contribution` | `get_contribution(env: Env, fund_id: Bytes, account: Address) -> i128` | none (view) | — | The account's voting power; `0` for a non-contributor |
| `get_allocations` | `get_allocations(env: Env, fund_id: Bytes) -> Vec<Allocation>` | none (view) | — | Oldest-first, capped at `HistoryLimit` |
| `get_growth` | `get_growth(env: Env, fund_id: Bytes) -> Vec<GrowthPoint>` | none (view) | — | Oldest-first, capped at `HistoryLimit` |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none | `hlth_alrt`, `rollback` | **Not a pure view**: writes `LastAlertLedger` and may auto-rollback the rollout state. No auth |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | — |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Compile-time constants; `env` unused |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` — **caller-supplied and never compared to `DataKey::Admin`** | `alrt_cfg` | The admin param is the only check; any address can sign for itself and rewrite thresholds |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to the shared default |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | — |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | Operator-supplied counter sample |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | — | — |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `feat_flg` | Appends to `FlagIndex` on first set |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | `false` once `RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `canary` | Aborts `"canary_bps exceeds 10000"` |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | none | — | Sticky `SHA-256(caller XDR) mod 10000 < canary_bps`; `caller` is unauthenticated, so advisory only |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | — |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rb_trig` | Aborts `"invalid rollback trigger"` for `0` or `> 10000` |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | — |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` — unverified against `DataKey::Admin` | `rollback`, `contract_paused` | Zeroes canary traffic, sets `Phase = RolledBack`, disables every indexed flag, and sets `shared::pause::PauseDataKey::Paused = true` — which this contract has no way to clear |

**Events.**

Eight contract-specific shapes (all `symbol_short!`) plus seven delegated from `shared`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `(admin: Address, history_limit: u32)` | `initialize` |
| `fund_new` | `(symbol_short!("fund_new"),)` | `(fund_id: Bytes, fund_type: FundType, steward: Address)` | `create_fund` — the `DistributionRule` is not in the payload, so the initial guardrails are only in storage |
| `rule` | `(symbol_short!("rule"),)` | `fund_id: Bytes` — a single value | `set_rule` — the new rule is not emitted, only the id |
| `contrib` | `(symbol_short!("contrib"),)` | `(fund_id: Bytes, contributor: Address, amount: i128)` | `contribute` — the resulting total is not included |
| `proposed` | `(symbol_short!("proposed"),)` | `(proposal_id: Bytes, fund_id: Bytes, amount: i128)` | `propose_allocation` — `recipient` and `memo` are omitted |
| `vote` | `(symbol_short!("vote"),)` | `(proposal_id: Bytes, voter: Address, support: bool, power: i128)` | `vote` |
| `finalize` | `(symbol_short!("finalize"),)` | `(proposal_id: Bytes, proposal.status: ProposalStatus)` | `finalize_proposal` |
| `executed` | `(symbol_short!("executed"),)` | `(proposal_id: Bytes, recipient: Address, amount: i128)` | `execute_allocation` |
| `contract_paused` | `(Symbol::new(env, "contract_paused"),)` | `shared::pause::ContractPausedEvent { admin: Address }` | `trigger_rollback` only |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check`, cooldown-gated |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `config.unhealthy_error_bps: u32` | `set_alert_config` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `error_bps: u32` | `set_rollback_trigger` |
| `rollback` | `(symbol_short!("rollback"),)` | `env.ledger().sequence(): u32` | `trigger_rollback`, and `health_check` via `maybe_auto_rollback` |

**Errors.**

`errors::FundError`, `#[contracterror]` with `#[repr(u32)]` and explicit `= N` discriminants; unit variants, with a `Display` impl and a `get_suggestion` mapping alongside.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` found `DataKey::Admin` already present in instance storage |
| `NotInitialized` | 2 | `require_initialized` found no `DataKey::Admin` — from every core entry point (`create_fund`, `set_rule`, `contribute`, `propose_allocation`, `vote`, `finalize_proposal`, `execute_allocation`) when called before `initialize` |
| `Unauthorized` | 3 | **Never constructed.** Declared and mapped to the `AUTH` suggestion, but no code path returns it — the steward's authority comes from `steward.require_auth()` against the stored `Fund` |
| `FundNotFound` | 4 | No `DataKey::Fund(id)` entry — from `get_fund`, and via `load_fund` inside `set_rule`, `contribute`, `propose_allocation`, `finalize_proposal` and `execute_allocation` |
| `FundExists` | 5 | `create_fund` found `DataKey::Fund(fund_id)` already present |
| `ProposalNotFound` | 6 | No `DataKey::Proposal(id)` entry — from `get_proposal`, and via `load_proposal` inside `vote`, `finalize_proposal`, `execute_allocation` |
| `ProposalExists` | 7 | `propose_allocation` found `DataKey::Proposal(proposal_id)` already present |
| `InvalidAmount` | 8 | `contribute` with `amount <= 0`; `propose_allocation` with `amount <= 0`; and `initialize` with `history_limit == 0` |
| `InvalidRule` | 9 | `validate_rule`: `max_allocation_bps == 0` or `> 10_000`, `quorum_bps > 10_000`, `voting_ledgers == 0`, or `min_reserve < 0` — from `create_fund` and `set_rule` |
| `NotContributor` | 10 | `propose_allocation` or `vote` where the caller's `Contribution(fund_id, account)` is `0` |
| `AlreadyVoted` | 11 | `vote` where `DataKey::Voted(proposal_id, voter)` already exists |
| `VotingClosed` | 12 | `vote` when the proposal status is not `Voting`, or `sequence > voting_ends_ledger`; also `finalize_proposal` when the status is not `Voting` |
| `VotingOpen` | 13 | `finalize_proposal` while `sequence <= voting_ends_ledger` |
| `ProposalNotApproved` | 14 | `execute_allocation` when the proposal status is anything other than `Approved` |
| `QuorumNotMet` | 15 | **Never constructed.** Defined and mapped to the `NO_QUORUM` suggestion, but `finalize_proposal` folds a quorum shortfall into `ProposalStatus::Rejected` and returns `Ok(Rejected)` |
| `ExceedsAllocationLimit` | 16 | `check_distribution_rule`: `amount > fund.balance * rule.max_allocation_bps / 10_000` — from `propose_allocation` and again from `execute_allocation` |
| `ReserveBreached` | 17 | `check_distribution_rule`: `fund.balance - amount < rule.min_reserve` — from `propose_allocation` and again from `execute_allocation`, which is how a post-approval `set_rule` blocks a payout |
| `ArithmeticOverflow` | 18 | `checked_add`/`checked_mul` failure on `fund.balance` or `total_contributed` in `contribute`, on `total_contributed * quorum_bps` or `balance * max_allocation_bps` in the rule checks, and on `total_allocated` in `execute_allocation` |

**Storage.** See [STORAGE.md](./STORAGE.md#creator-fund).

**Compile status.** `compiles` — `cargo check -p creator_fund` and `cargo test -p creator_fund --no-run` both succeed with no warnings. The 17 unit tests in `src/test.rs` compile against the `rlib` target.


### dao

> This crate has no `//!` module doc comment. Nearest header comment (contracts/dao/src/lib.rs:1-9): "Implements DAO Governance Contract — closes #615. Acceptance Criteria: Implement voting mechanism (1 token = 1 vote); Support proposal creation and discussion; Add timelock for execution; Implement multi-sig validation; Track governance history."

Source: `contracts/dao/`

**Purpose.**

On-chain governance for a token-holder collective: anyone holding at least one governance token can open a proposal, and every holder's vote is weighted by their live governance-token balance (1 token = 1 vote, snapshotted at vote time, one vote per address per proposal). After a proposal's voting window closes, anyone may `tally` it; passing requires both a 10% quorum of total token supply and a strict majority of cast votes, after which a timelock must elapse before the admin can mark it `Executed`. Proposals flagged `requires_multisig` additionally need `MultiSigThreshold` approvals from the `MultiSigSigners` list. The contract is deliberately a *registry*, not an executor: `execute` only records the outcome, and the actual call to the target contract must be made separately off-contract with the proposal's encoded calldata. There is no treasury, no admin rotation, and no post-initialisation way to change the signer set, threshold, timelock, or voting period.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` (declared but never referenced in `src/lib.rs` — no `shared::` path appears in the source). Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to 21.7.7 in `Cargo.lock`); dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, gov_token: Address, multisig_signers: Vec<Address>, multisig_threshold: u32, timelock_ledgers: u32, voting_period: u32)` | `admin.require_auth()` | `init` | One-shot setup. Asserts `multisig_threshold <= multisig_signers.len()` and `voting_period > 0`. Maps `timelock_ledgers == 0` to `DEFAULT_TIMELOCK_LEDGERS`. Seeds `ProposalCount` and `HistoryCount` to `0`. **Not idempotent-guarded** — a second call silently overwrites the admin and token. |
| `create_proposal` | `fn create_proposal(env: Env, proposer: Address, title: String, description: String, requires_multisig: bool) -> u64` | `proposer.require_auth()` | `proposed` | Any token holder may propose: asserts `tok.balance(&proposer) >= 1` ("proposer must hold at least 1 governance token"). Sets `voting_start = env.ledger().sequence()`, `voting_end = now + VotingPeriodLedgers`, `executable_after = 0`, `status = Active`, `multisig_approvals = 0`. Returns the new id. |
| `cancel_proposal` | `fn cancel_proposal(env: Env, caller: Address, proposal_id: u64)` | `caller.require_auth()`, plus role check `caller == proposal.proposer \|\| caller == admin` | `cancelled` | Only `Active` proposals can be cancelled. Sets `status = Cancelled`. |
| `cast_vote` | `fn cast_vote(env: Env, voter: Address, proposal_id: u64, support: bool)` | `voter.require_auth()` | `voted` | One vote per address per proposal (`HasVoted` guard). Requires `status == Active` and `now <= voting_end`. Weight = live `tok.balance(&voter)` at vote time; asserts `weight > 0`. `support == true` adds to `votes_for`, else to `votes_against`. Persists `HasVoted`, a `VoteRecord`, and the updated `Proposal`. |
| `tally` | `fn tally(env: Env, proposal_id: u64)` | `none (permissionless — any address may call; no `require_auth`)` | `tallied` | **DOES NOT COMPILE** — see Compile status. Intended behaviour: requires `status == Active` and `now > voting_end`; passes when `(votes_for + votes_against) >= (total_supply * 1_000 / 10_000) && votes_for > votes_against`. On pass sets `status = Queued` and `executable_after = now + TimelockLedgers`; on fail sets `status = Defeated`. Records history attributed to the stored `Admin` as a "system action". |
| `approve_multisig` | `fn approve_multisig(env: Env, signer: Address, proposal_id: u64)` | `signer.require_auth()`, plus role check `signers.contains(&signer)` against `DataKey::MultiSigSigners` | `msapprove` | Requires `status == Queued` **and** `requires_multisig == true`; guards against double approval via `MultiSigApproval`. Increments `multisig_approvals`. Only when `multisig_approvals >= MultiSigThreshold` (fallback `1`) **and** `now >= executable_after` in the *same* call does it set `status = Ready`. If the threshold is met before the timelock matures, the proposal stays `Queued` and no further transition is possible, because every signer is already flagged as having approved. |
| `execute` | `fn execute(env: Env, admin: Address, proposal_id: u64)` | `admin.require_auth()`, plus `admin == stored_admin` | `executed` | Admin-only. For `requires_multisig` proposals the status must be exactly `Ready`; otherwise it must be `Queued` or `Ready`. Asserts `now >= proposal.executable_after`. Sets `status = Executed` **only** — no external call, no token movement, no config mutation. |
| `get_proposal` | `fn get_proposal(env: Env, proposal_id: u64) -> Proposal` | none (view) | — | Loads `DataKey::Proposal(proposal_id)`; panics `"proposal not found"` if absent. |
| `has_voted` | `fn has_voted(env: Env, proposal_id: u64, voter: Address) -> bool` | none (view) | — | Reads `HasVoted`, defaulting to `false`. |
| `get_vote` | `fn get_vote(env: Env, proposal_id: u64, voter: Address) -> Option<VoteRecord>` | none (view) | — | Reads `Vote`; returns `None` when no vote was cast. |
| `get_history` | `fn get_history(env: Env, offset: u64, limit: u64) -> Vec<GovernanceEvent>` | none (view) | — | Paginated slice of the history log, bounded by `(offset + limit).min(HistoryCount)`. |

Internal (not part of the external interface): `next_proposal_id`, `load_proposal`, `record_history`.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `admin` | `initialize` |
| `proposed` | `[contract, "proposed"]` | `(id, proposer, title, requires_multisig)` | `create_proposal` |
| `cancelled` | `[contract, "cancelled"]` | `(proposal_id, caller)` | `cancel_proposal` |
| `voted` | `[contract, "voted", proposal_id]` | `(voter, support, weight)` | `cast_vote` |
| `tallied` | `[contract, "tallied", proposal_id]` | `(passed, votes_for, votes_against)` | `tally` (unreachable — does not compile) |
| `msapprove` | `[contract, "msapprove", proposal_id]` | `(signer, multisig_approvals)` | `approve_multisig` |
| `executed` | `[contract, "executed"]` | `(proposal_id, admin)` | `execute` |

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| no `#[contracterror]` enum — all failures are host panics | n/a | Every rejection is a string-literal `assert!` or an `Option::expect`. In `initialize`: `"threshold cannot exceed signer count"`, `"voting period must be > 0"`. In `create_proposal`: `expect("not initialized")` for `GovToken`/`VotingPeriodLedgers`, `"proposer must hold at least 1 governance token"`. In `cancel_proposal`: `expect("not initialized")`, `"only proposer or admin can cancel"`, `"only active proposals can be cancelled"`. In `cast_vote`: `"voter has already voted on this proposal"`, `expect("proposal not found")`, `"proposal is not active"`, `"voting period has ended"`, `expect("not initialized")`, `"voter must hold governance tokens to vote"`. In `tally`: `expect("proposal not found")`, `"proposal is not active"`, `"voting period not yet ended"`, `expect("not initialized")`. In `approve_multisig`: `expect("not initialized")`, `"caller is not a registered multi-sig signer"`, `expect("proposal not found")`, `"proposal is not in Queued state"`, `"proposal does not require multi-sig"`, `"signer has already approved this proposal"`. In `execute`: `expect("not initialized")`, `"only admin can execute"`, `expect("proposal not found")`, `"multi-sig proposal must be in Ready state"`, `"proposal is not executable"`, `"timelock has not yet expired"`. |

**Storage.** See [STORAGE.md](./STORAGE.md#dao).

**Compile status.** `fails to compile`  > **Compile status:** `cargo build -p dao` fails with exactly one error: `error[E0599]: no method named 'total_supply' found for struct 'Client<'a>' in the current scope` at `contracts/dao/src/lib.rs:352:32` (`let total_supply = tok.total_supply();`, inside `tally`). The `tok` value is a `soroban_sdk::token::Client` (alias of `TokenClient`, generated from the SEP-41 `TokenInterface` trait in soroban-sdk 21.7.7), which exposes only `allowance`, `approve`, `balance`, `transfer`, `transfer_from`, `burn`, `burn_from`, `decimals`, `name`, and `symbol` — SEP-41 defines no `total_supply` function, so the call cannot be satisfied. Consequently: the entire crate produces no `cdylib`/`rlib` artifact, so every other function (`initialize`, `create_proposal`, `cancel_proposal`, `cast_vote`, `approve_multisig`, `execute`, `get_proposal`, `has_voted`, `get_vote`, `get_history`) is unreachable in practice and no `DaoGovernanceClient` or WASM can be built or deployed. Within `tally` specifically, everything downstream of line 352 is unreachable: the quorum computation `quorum = (total_supply * QUORUM_BPS) / 10_000`, the `passed` predicate, the `Active → Queued` and `Active → Defeated` transitions, the assignment `executable_after = now + timelock`, the final `Proposal` write, the `"queued"`/`"defeated"` history record, and the `tallied` event. Because `tally` is the *only* writer of `executable_after`, the whole downstream governance path is dead: `approve_multisig` can never observe `status == Queued` (so the multi-sig path and the `Queued → Ready` transition are unreachable), and `execute`'s `now >= executable_after` check can only ever be satisfied against the `0` written at creation — reachable in principle for a non-`requires_multisig` proposal, but unreachable in practice since no artifact builds. There is no test module in this crate, so no test failure masks the build error.


### dispute_arbiter

> "Dispute Arbiter Smart Contract — Autonomous arbitration and dispute settlement for StellarAid escrows. Architecture Decision: [ADR-0004](../../docs/ADRs/0004-dispute-resolution-and-arbitration.md)"

Source: `contracts/dispute_arbiter/`

**Purpose.**

Arbitrates disputes that arise over StellarAid escrow commissions keyed by an opaque `Bytes` `commission_id`. A dispute is opened by a self-authorising `initiator`; from then on settlement is admin-driven — the arbiter invokes the escrow contract's `refund_cl` or `rel_pay` entry points and records a `DisputeRecord` with a status and a free-text note. `partial_resolve` supports a split settlement: it refunds the client via the escrow contract, reads the escrow's live token balance from the token returned by `config_contract.get_usdc`, and then transfers the two computed bps shares directly from the escrow contract address. If nobody acts, `auto_resolve` becomes callable after a configured number of ledgers and defaults to a full client refund. The crate also re-exports the shared health-monitoring (#678) and gradual-rollout (#684) surface on every public entry point.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` (actively used for `shared::health` and `shared::rollout`). Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to 21.7.7 in `Cargo.lock`); dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. No other dependencies. Additionally, `src/lib.rs:15` does `include!("../../semver_types.rs")`, textually splicing the repo-level `contracts/semver_types.rs` (not a crate dependency) into the crate.

**Public interface.**

The last three rows are installed by `impl_semver_queries!()` from `include!`-ed `contracts/semver_types.rs` at `src/lib.rs:118`; they are inside the `#[contractimpl]` block and so are part of the exported contract spec/client.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, escrow_contract: Address, config_contract: Address, auto_resolve_ledgers: u32) -> Result<(), DisputeError>` | `none — no `require_auth()` is called on `admin`; guard is only "already initialised" | `init` | Rejects a second call with `AlreadyInitialized`. Writes the four instance keys. Anyone can front-run initialisation, since nothing authorises `admin`. |
| `open_dispute` | `fn open_dispute(env: Env, commission_id: Bytes, initiator: Address) -> Result<(), DisputeError>` | `initiator.require_auth()` | `opened` | Requires initialisation. A pre-existing `Dispute(commission_id)` yields `AlreadyResolved` (used as the "already exists" error). Writes `DisputeStatus::Open` with `opened_ledger = env.ledger().sequence()` and `auto_resolve_ledger = opened_ledger + auto_resolve_ledgers`. Does **not** call the escrow contract's `open_dis`. |
| `resolve_for_client` | `fn resolve_for_client(env: Env, commission_id: Bytes, note: String) -> Result<(), DisputeError>` | `admin.require_auth()` (the `Admin` read from storage) | `resolved` | Requires `status == DisputeStatus::Open`, else `InvalidStatus`. Invokes escrow `refund_cl(commission_id, config_contract)`, then sets `ResolvedForClient` and stores `note`. |
| `resolve_for_artist` | `fn resolve_for_artist(env: Env, commission_id: Bytes, note: String) -> Result<(), DisputeError>` | `admin.require_auth()` (the `Admin` read from storage) | `resolved` | Same shape as `resolve_for_client` but invokes escrow `rel_pay(commission_id, config_contract)` and sets `ResolvedForArtist`. |
| `partial_resolve` | `fn partial_resolve(env: Env, commission_id: Bytes, client_share_bps: u32, note: String) -> Result<(), DisputeError>` | `admin.require_auth()` (the `Admin` read from storage) | `resolved` | Rejects `client_share_bps > 10000` with `InvalidShareBps`; `artist_share_bps = 10000 - client_share_bps` via `checked_sub`. Requires `status == Open`. First invokes escrow `refund_cl` to move funds under this contract's auth, then reads `usdc = config.get_usdc()` and `escrow_balance = usdc.balance(escrow_contract)`, then issues up to two `usdc.transfer(escrow_contract, commission_id, share)` calls. Share amounts are `escrow_balance * bps / 10000` with integer truncation. Sets `PartiallyResolved` and stores `note`. |
| `auto_resolve` | `fn auto_resolve(env: Env, commission_id: Bytes) -> Result<(), DisputeError>` | `none (permissionless — no `require_auth()`; guard is "initialised" plus elapsed ledgers)` | `auto_res` | Requires `status == Open`. Rejects with `AutoResolveNotDue` while `env.ledger().sequence() < record.auto_resolve_ledger`. On success invokes escrow `refund_cl` (full client refund) and sets `AutoResolved`. Leaves `resolution_note` as `None`. |
| `get_dispute` | `fn get_dispute(env: Env, commission_id: Bytes) -> Result<DisputeRecord, DisputeError>` | none (view) | — | Returns `NotFound` when `Dispute(commission_id)` is absent. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt` (conditionally), `rollback` (conditionally) | Delegates to `shared::health::health_check`, which classifies status and may publish `hlth_alrt` subject to the alert cooldown. If `report.anomaly` is true, also calls `shared::rollout::maybe_auto_rollback`, which can publish `rollback` and mutate rollout keys. This is a state-mutating call despite its query-like signature. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | `paused` field is derived from `shared::pause::PauseDataKey::Paused`, not from `HealthKey::Metrics`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Returns pure constants; `let _ = env;` discards the env. |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `alrt_cfg` | **No stored-admin comparison** — any address can overwrite the alert config, and `health.rs` will `panic!("invalid alert config")` on bps > 10 000, inverted thresholds, or a zero stall/cooldown value. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Returns `default_alert_config()` when unset. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | Pure classification; emits nothing. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `ok_count` (saturating) and sets `last_ok_ledger`. No stored-admin comparison. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `error_count` (saturating) and sets `last_error_ledger`. No stored-admin comparison. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool)` | `admin.require_auth()` | `feat_flg` | Upserts `RolloutKey::Flag(flag)` and appends the flag to `FlagIndex` if new. No stored-admin comparison. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool` | none (view) | — | Returns `false` unconditionally once the phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `canary` | Persists canary/stable/bps and derives the phase; `panic!("canary_bps exceeds 10000")` if out of range. No stored-admin comparison. |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky split: `false` when phase is `RolledBack` or bps is 0, `true` at bps ≥ 10 000, otherwise `SHA-256(caller XDR)[0..4]` as big-endian `u32` mod 10 000 `< canary_bps`. Read-only despite the name. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | Returns phase, bps, optional canary/stable, `rollback_error_bps` (default 500), and `flag_count`. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `rb_trig` | `panic!("invalid rollback trigger")` if `error_bps == 0` or `> 10 000`. No stored-admin comparison. |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | `true` when the phase is already `RolledBack`, else `health::error_bps(metrics) >= rollback_error_bps && (ok_count + error_count) > 0`. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` | `rollback`, `contract_paused` | Zeroes `CanaryBps`, sets phase `RolledBack`, disables every indexed flag, and sets `PauseDataKey::Paused = true`. No stored-admin comparison; no way to un-pause from this crate. |
| `get_version` | `fn get_version(_env: soroban_sdk::Env) -> ContractVersion` | none (view) | — | Macro-generated; parses `CARGO_PKG_VERSION` (0.1.0) into `{ major, minor, patch }`. No storage. |
| `get_version_metadata` | `fn get_version_metadata(env: soroban_sdk::Env) -> VersionMetadata` | none (view) | — | Macro-generated; `{ name, version, min_compatible, storage_schema }` with `storage_schema = CURRENT_STORAGE_SCHEMA` (1) and `min_compatible` = `{0, 1, 0}`. |
| `is_version_compatible` | `fn is_version_compatible(_env: soroban_sdk::Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated; major must match exactly; for major 0 the minor must match and the required patch must be `<=` current. |

Internal (free functions, not in `#[contractimpl]`): `has_admin`, `get_admin`, `get_escrow_contract`, `get_config_contract`, `get_auto_resolve_ledgers`, `dispute_exists`, `load_dispute`, `save_dispute`.

**Events.**

Topics are listed as `[contract address, …]`. The first five rows are published in this crate; the rest are published by `shared` but are reachable through this crate's public entry points (marked in "Emitted by").

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, escrow_contract, config_contract, auto_resolve_ledgers)` | `initialize` |
| `opened` | `[contract, "opened"]` | `(commission_id, initiator, current_ledger, auto_resolve_ledger)` | `open_dispute` |
| `resolved` (3-tuple shape) | `[contract, "resolved"]` | `(commission_id, DisputeStatus, note)` — status is `ResolvedForClient` or `ResolvedForArtist` | `resolve_for_client`, `resolve_for_artist` |
| `resolved` (4-tuple shape) | `[contract, "resolved"]` | `(commission_id, DisputeStatus, client_share_bps, note)` — status is `PartiallyResolved` | `partial_resolve` |
| `auto_res` | `[contract, "auto_res"]` | `(commission_id, current_ledger)` | `auto_resolve` |
| `alrt_cfg` | `[contract, "alrt_cfg"]` | `unhealthy_error_bps` | `shared::health::set_alert_config`, via `set_alert_config` |
| `hlth_alrt` | `[contract, "hlth_alrt"]` | `(status, error_bps, stalled)` — only when `alerting_enabled` and an anomaly is present, rate-limited by the cooldown | `shared::health::health_check`, via `health_check` |
| `canary` | `[contract, "canary"]` | `(canary, stable, canary_bps)` | `shared::rollout::set_canary_deployment`, via `set_canary_deployment` |
| `feat_flg` | `[contract, "feat_flg"]` | `(flag, enabled)` | `shared::rollout::set_feature_flag`, via `set_feature_flag` |
| `rb_trig` | `[contract, "rb_trig"]` | `error_bps` | `shared::rollout::set_rollback_trigger`, via `set_rollback_trigger` |
| `rollback` | `[contract, "rollback"]` | `env.ledger().sequence()` | `shared::rollout::apply_rollback`, via `trigger_rollback` and via `health_check` → `maybe_auto_rollback` |
| `contract_paused` | `[contract, "contract_paused"]` (full `Symbol`, not `symbol_short!`) | `shared::pause::ContractPausedEvent { admin }` | `shared::rollout::trigger_rollback`, via `trigger_rollback` |

**Errors.**

`#[contracterror] pub enum DisputeError` in `src/errors.rs`, `#[repr(u32)]`, with a `Display` impl and a `get_suggestion` mapping to a `Symbol`.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` called when `DataKey::Admin` already exists. |
| `NotInitialized` | 2 | `open_dispute` or `auto_resolve` called before initialisation, or any of the `get_*` instance helpers invoked when `has_admin` is false. |
| `Unauthorized` | 3 | Never constructed anywhere in `src/lib.rs` — declared and documented (`"unauthorized"`, suggestion `AUTH`) but dead. Actual authorisation failures surface as `HostError` from `require_auth`. |
| `NotFound` | 4 | `load_dispute` when `Dispute(commission_id)` does not exist. |
| `InvalidStatus` | 5 | A resolution path called on a dispute whose `status` is not `Open` (i.e. already `ResolvedForClient`, `ResolvedForArtist`, `PartiallyResolved`, or `AutoResolved`). |
| `AlreadyResolved` | 6 | `open_dispute` for a `commission_id` that already has a dispute record. |
| `AutoResolveNotDue` | 7 | `auto_resolve` before `record.auto_resolve_ledger`. |
| `InvalidShareBps` | 8 | `partial_resolve` with `client_share_bps > 10000`, or if the `checked_sub` for the artist share fails. |
| `ArithmeticOverflow` | 9 | Never constructed anywhere in `src/lib.rs` — declared and documented but dead; the arithmetic in `partial_resolve` uses unchecked `*` and `/` on `i128`. |

**Storage.** See [STORAGE.md](./STORAGE.md#dispute-arbiter).

**Compile status.** `compiles` — `cargo build -p dispute_arbiter` finishes cleanly (no warnings emitted).


### ecosystem_funding

> unknown — `contracts/ecosystem_funding/src/lib.rs` has no crate-level `//!` module doc; it opens with plain `//` header comments ("Implements Ecosystem Funding Programs — closes #617", followed by the five acceptance criteria). The summary below is reconstructed from that header plus the source.

Source: `contracts/ecosystem_funding/`

**Purpose.**

`ecosystem_funding` runs grant/incubator/accelerator programmes where money moves in tranches against delivered work. A manager opens a programme for a named grantee, splitting the total into milestone tuples at creation; the recipient submits each milestone with a deliverable URI, and the manager approves or rejects it — approval transfers that milestone's tokens from the contract to the recipient, and the programme auto-completes when the last milestone is approved. Recipients also file performance metrics (users onboarded, revenue, and so on) against the programme, stored as individually addressable, paginated records. It is aimed at programme operators who want milestone-based disbursement and outcome tracking on-chain, and it is the odd one out in this group: it has no shared pause, no health/rollout surface, no semver queries, and no `#[contracterror]` enum — every failure is a string assertion.

**Dependencies.**

Path dep: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — **declared but never used**; `lib.rs` contains no reference to the `shared` crate, so none of `shared::pause`, `shared::health`, `shared::rollout` or `shared::version` storage appears in this contract. Registry dep: `soroban-sdk` `21.0.0` (workspace-inherited; dev-dependency adds `testutils`; a pass-through `testutils` feature is declared). `version` and `edition` are workspace-inherited (`0.1.0`, `2021`). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `[lib] crate-type = ["cdylib", "rlib"]`. There is no `src/test.rs` and no other test module, so this crate ships no unit tests.

**Public interface.**

11 `pub fn`s in the single `#[contractimpl] impl EcosystemFunding` block. The private `fn`s — `next_id`, `load_program`, `require_manager_or_admin` — are internal and omitted. The role check is: caller must equal the programme's `manager` **or** the stored `admin`, else the assert `"caller must be manager or admin"` fires.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address)` | `admin.require_auth()` | `init` | Stores `Admin` and `ProgramCount = 0`. **No re-initialization guard**: a second call silently overwrites the admin and resets the programme counter to 0, orphaning existing `DataKey::Program` records. `load_program`/the role check read `Admin` with `.expect("not initialized")` |
| `create_program` | `create_program(env: Env, manager: Address, recipient: Address, name: String, description: String, program_type: ProgramType, milestones: Vec<(String, String, i128)>, token: Address) -> u64` | `manager.require_auth()` | `created` | Returns the new 1-based ID. `total_allocation` is the sum of the milestone amounts; asserts `"at least one milestone required"` on an empty list and `"milestone disbursement must be positive"` per milestone. No admin/manager validation is done against `DataKey::Admin` here, and the programme is created in `Active` with `total_disbursed = 0`. Milestones are stored as a single vector with `index` set positionally and `deliverable_uri` initialised to the empty string |
| `pause_program` | `pause_program(env: Env, caller: Address, program_id: u64)` | `caller.require_auth()` + caller is the programme's `manager` or the stored `admin` | `paused` | Asserts `"program is not active"` unless status is `Active`, then sets `Paused`. Reversible only by `resume_program` |
| `resume_program` | `resume_program(env: Env, caller: Address, program_id: u64)` | `caller.require_auth()` + manager or admin | `resumed` | Asserts `"program is not paused"` unless status is `Paused`, then sets `Active` |
| `cancel_program` | `cancel_program(env: Env, manager: Address, program_id: u64)` | `manager.require_auth()` + manager or admin | `cancelled` | Asserts `"cannot cancel a completed or already-cancelled program"` unless status is `Active` or `Paused`. Stops further disbursement but does **not** claw back tokens already transferred, and there is no `Paused → Cancelled` path (a paused programme can only be resumed) |
| `submit_milestone` | `submit_milestone(env: Env, recipient: Address, program_id: u64, milestone_index: u32, deliverable_uri: String)` | `recipient.require_auth()` + `recipient` must equal the programme's `recipient` | `ms_sub` | Asserts `"caller is not the program recipient"` and `"program is not active"` (a `Paused` programme cannot accept submissions), then `"milestone already submitted or approved"` unless the status is `Pending`. Records `deliverable_uri` and `submitted_at`. No URI length limit |
| `review_milestone` | `review_milestone(env: Env, manager: Address, program_id: u64, milestone_index: u32, approve: bool)` | `manager.require_auth()` + manager or admin | `ms_rev`, and `complete` when the last milestone is approved | Asserts `"program is not active"` and `"milestone must be in Submitted state"`. On `approve`: `token::transfer(contract → programme.recipient, ms.disbursement_amount)`, `total_disbursed +=` the same amount, status `Approved` with `approved_at`; when every milestone is `Approved` the programme flips to `Completed` and `complete` is published. On reject: status `Rejected` with no transfer, and the rejected milestone can never be resubmitted |
| `record_outcome` | `record_outcome(env: Env, recipient: Address, program_id: u64, metric_name: String, metric_value: i128, description: String)` | `recipient.require_auth()` + `recipient` must equal the programme's `recipient` | `outcome` | Appends a `ProgramOutcome` at index `OutcomeCount(program_id)` and increments that counter. There is no status gate (an outcome can be filed on a `Paused`, `Completed` or even `Cancelled` programme) and no check on `metric_value`; the doc comment gives `("users_onboarded", 150)` and `("revenue_usd_cents", 120000)` as examples |
| `get_program` | `get_program(env: Env, program_id: u64) -> FundingProgram` | none (view) | — | `.expect("program not found")`, so an unknown ID aborts rather than returning `Option` |
| `get_milestones` | `get_milestones(env: Env, program_id: u64) -> Vec<ProgramMilestone>` | none (view) | — | `.expect("milestones not found")` — panics for an unknown programme even though `create_program` always writes the vector |
| `get_outcomes` | `get_outcomes(env: Env, program_id: u64, offset: u64, limit: u64) -> Vec<ProgramOutcome>` | none (view) | — | Paginated over `[offset, min(offset + limit, count))`, skipping indices whose entry is missing. A `limit` of `0` returns an empty vector, and an `offset` past the end returns an empty vector rather than aborting |

**Events.**

Nine shapes. The first five use a single `symbol_short!` topic; the four milestone/outcome events use a second `u64` programme id as a **second topic**, so they are the only ones in this crate with two-element topic tuples.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `admin: Address` — a single value, not a tuple | `initialize` |
| `created` | `(symbol_short!("created"),)` | `(id: u64, manager: Address, recipient: Address, name: String, program_type: ProgramType)` | `create_program` — `total_allocation` and the milestone list are not in the payload |
| `paused` | `(symbol_short!("paused"),)` | `(program_id: u64, caller: Address)` | `pause_program` |
| `resumed` | `(symbol_short!("resumed"),)` | `(program_id: u64, caller: Address)` | `resume_program` |
| `cancelled` | `(symbol_short!("cancelled"),)` | `(program_id: u64, manager: Address)` | `cancel_program` |
| `ms_sub` | `(symbol_short!("ms_sub"), program_id: u64)` | `(milestone_index: u32, recipient: Address, deliverable_uri: String)` | `submit_milestone` — the milestone status and title are not in the payload |
| `ms_rev` | `(symbol_short!("ms_rev"), program_id: u64)` | `(milestone_index: u32, approve: bool)` | `review_milestone` — the approved amount is not in the payload; the token `transfer` is not represented as an event either |
| `complete` | `(symbol_short!("complete"),)` | `program_id: u64` — a single value | `review_milestone`, only on the approval that turns the last milestone `Approved` |
| `outcome` | `(symbol_short!("outcome"), program_id: u64)` | `(recipient: Address, metric_name: String, metric_value: i128, description: String)` | `record_outcome` — the outcome's own index (`ledger`) is not in the payload |

**Errors.**

None. The crate declares no `#[contracterror]` enum and has no `src/errors.rs`; every rejection is a `assert!`, `.expect(...)` or `.get(...).expect(...)` with a string literal, so callers receive an opaque `HostError` with no code to branch on: `"at least one milestone required"`, `"milestone disbursement must be positive"`, `"program is not active"`, `"program is not paused"`, `"cannot cancel a completed or already-cancelled program"`, `"caller is not the program recipient"`, `"milestones not found"`, `"milestone index out of range"`, `"milestone already submitted or approved"`, `"milestone must be in Submitted state"`, `"program not found"`, `"not initialized"`, `"caller must be manager or admin"`. There is also no `get_version` / `get_version_metadata` / `is_version_compatible` surface — this crate is the only one of the four that does not include `semver_types.rs`.

| Variant | Code | Condition |
|---|---|---|
| none | — | unknown — the crate has no `#[contracterror]` enum; all failures are string-literal `assert!`/`expect` aborts listed above |

**Storage.** See [STORAGE.md](./STORAGE.md#ecosystem-funding).

**Compile status.** `compiles` — `cargo check -p ecosystem_funding` and `cargo test -p ecosystem_funding --no-run` both succeed with no warnings. There are no tests to run: the crate has no `#[cfg(test)]` module and no `src/test.rs`, so the test target is empty.


### escrow

> Handles locking, releasing, refunding, and dispute escrow workflows for StellarAid.

Source: `contracts/escrow/`

**Purpose.**

Escrow is the platform's custody contract: it holds a client's USDC in the contract's own token balance between commission creation and settlement, so neither side can move the money unilaterally. It resolves the fee basis points, USDC address, admin and platform wallet from the `PlatformConfig` contract at call time rather than storing them, and pays the artist and the platform wallet on release, partial release, cancellation, deadline auto-release, or atomic migration to a commission. It also carries the surrounding operational machinery: a pause flag with two-step admin key rotation, a re-entrancy guard, ledger-based TTL extension of escrow records, dispute opening with an arbitration-length TTL, and pass-through endpoints for the shared health-monitoring and canary-rollout modules. The intended state machine is `Locked → {Released, Refunded, Disputed, Expired, Cancelled}` with `PartiallyReleased` as an intermediate on the way to `Released`.

**Dependencies.**

| Dependency | Kind | Version |
|---|---|---|
| `soroban-sdk` | registry | `21.0.0` (workspace pin), plus `21.0.0` with `testutils` as a dev-dependency |
| `shared` | path (`../shared`) | `0.1.0` |
| `../../semver_types.rs` | `include!` (file, not a Cargo dep) | provides `ContractVersion`, `VersionMetadata`, `CURRENT_STORAGE_SCHEMA = 1`, `parse_pkg_semver`, `is_compatible`, `min_compatible_for` |

Crate is `#![no_std]`, `crate-type = ["cdylib", "rlib"]`, `publish = false`, single feature `testutils`. `shared` is used for `correlation`, `health`, and `rollout`.

**Modules.**

11 modules are declared in `lib.rs` (3 real + 8 `#[cfg(test)]`); `asset_registry.rs` and `token.rs` exist on disk but are **never declared**, so they are dead files that do not compile as part of the crate. A `benches/` file is also present but has no `[[bench]]` target in `Cargo.toml`.

| Module | Responsibility |
|---|---|
| `lib.rs` | Crate root: `EscrowContract` (`#[contract]`), the single `#[contractimpl]` block with all 45 `pub fn`s, `PauseKey` + pause helpers, `ESCROW_TTL_LEDGERS`, `extend_escrow_ttl`/`extend_escrow_ttl_default`, `calculate_fee_split`, `correlation_publish`, and the 8 test-module declarations. |
| `errors.rs` | `#[contracterror] EscrowError` (17 variants, codes 1–17), its `Display` impl, and `get_suggestion()` → short symbol. |
| `storage.rs` | `CommissionStatus` enum, `EscrowRecord` struct, `DataKey` enum, all `env.storage()` read/write helpers (`escrow_exists`, `get_escrow`, `save_escrow`, atomic-marker helpers), the re-entrancy guard (`is_locked`/`set_locked`/`clear_locked`/`with_reentrancy_guard`), and `DEFAULT_DISPUTE_TTL_LEDGERS` + dispute-TTL get/set. |
| `cross_contract.rs` | PlatformConfig getters (`get_fee_bps`, `get_usdc_token`, `get_admin`, `get_platform_wallet`), USDC helpers (`usdc_transfer`, `usdc_balance`, `check_sufficient_balance`), `ConfigBundle`, the DisputeArbiter → escrow call helpers, and the whole atomic escrow→commission orchestration (`AtomicCommitState`, `AtomicCommitMarker`, `begin/confirm/finalize/rollback/verify/atomic_escrow_to_commission`). |
| `tests.rs` (`#[cfg(test)]`) | Baseline status/error-discriminant assertions, CEI documentation tests, `ESCROW_TTL_LEDGERS == 432_000`, `PartiallyReleased` transition logic, version-metadata test. |
| `refund_tests.rs` (`#[cfg(test)]`) | create→refund state-machine assertions (status-level only, no `Env` balance checks). |
| `dispute_tests.rs` (`#[cfg(test)]`) | `open_dispute` eligibility/authorization/idempotency assertions at status level. |
| `fee_math_tests.rs` (`#[cfg(test)]`) | Direct unit tests of `calculate_fee_split`, including `i128::MAX` overflow boundaries. |
| `storage_edge_tests.rs` (`#[cfg(test)]`) | Real `Env` storage round-trips (empty/max/negative values, `i128::MAX`, 256-byte ids), re-entrancy guard lifecycle, dispute-TTL defaults/bounds. |
| `cancellation_tests.rs` (`#[cfg(test)]`) | Full `MockConfig` + SAC fixture; exercises `cancel_escrow` split, fee-only-on-artist-share, exact drain, `InvalidSplit`, `InvalidAmount`, disputed-escrow cancellation, repeat rejection. |
| `integration_tests.rs` (`#[cfg(test)]`) | `Display` string and `get_suggestion` mapping checks, lifecycle discriminant checks, discriminant-uniqueness check (11 variants). |
| `atomic_flow_tests.rs` (`#[cfg(test)]`) | `MockConfig` + `MockCommission` fixtures; begin/confirm/finalize/rollback, consistency probe, and the single-transaction migration incl. atomic failure with zero fund movement. |
| `correlation_tests.rs` (`#[cfg(test)]`) | `shared::correlation` determinism/linking/round-trip tests plus escrow-side assertions that `create_escrow` emits `(escrow, created, corr)` alongside the unchanged 2-topic primary events. |
| `asset_registry.rs` (**unreferenced**) | `EscrowAssetRegistry::validate_registry_at_init` (hardcoded `true`) and `calculate_fee_deduction` (unchecked `amount * fee_bps / 10000`). Not declared as a module. |
| `token.rs` (**unreferenced**) | A single `soroban_sdk::contractimport!` of a prebuilt `soroban_token_contract.wasm`. Not declared as a module. |
| `benches/escrow_benchmark.rs` (**not built**) | `#![cfg(test)]` + `extern crate test` benches for create/dispute/release/refund/get. No `[[bench]]` target exists, it is gated to nightly-only `#[bench]`, and it imports crate name `escrow_contract` while the package is named `escrow`. |

**Public interface.**

All 45 rows come from the single `#[contractimpl] impl EscrowContract` block at `lib.rs:98–825` (there is no second block; the `MockConfig`/`MockCommission` `#[contractimpl]`s at `cancellation_tests.rs:18`, `correlation_tests.rs:16` and `atomic_flow_tests.rs:21/47` are test doubles, not escrow exports). Rows 20–45 fall inside the unclosed `auto_release_on_deadline` body and are not compilable — see Compile status.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `(env: Env, admin: Address) -> Result<(), EscrowError>` | `admin.require_auth()` | — | One-shot. `AlreadyExists` if `PauseKey::Admin` already set. Writes `Admin` + `Paused = false`. |
| `get_version` | `(env: Env) -> ContractVersion` | none (view) | — | Parses `CARGO_PKG_VERSION`; parameter is named `_env` and unused. |
| `get_version_metadata` | `(env: Env) -> VersionMetadata` | none (view) | — | Name, version, `min_compatible`, `storage_schema = 1`. |
| `is_version_compatible` | `(env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | `env` named `_env`, unused. |
| `pause` | `(env: Env, admin: Address) -> Result<(), EscrowError>` | `admin.require_auth()` + `admin == PauseKey::Admin` | `(esc, paused)` | Sets `PauseKey::Paused = true`; blocks only `create_escrow` and `refund_client`. |
| `unpause` | `(env: Env, admin: Address) -> Result<(), EscrowError>` | `admin.require_auth()` + `admin == PauseKey::Admin` | `(esc, unpaused)` | Clears the flag. |
| `is_paused` | `(env: Env) -> bool` | none (view) | — | Defaults to `false` when unset. |
| `transfer_admin` | `(env: Env, admin: Address, new_admin: Address) -> Result<(), EscrowError>` | `admin.require_auth()` + `admin == PauseKey::Admin` | `(esc, adm_prop)` | Step 1 of rotation; writes `PendingAdmin` only. |
| `accept_admin` | `(env: Env, new_admin: Address) -> Result<(), EscrowError>` | `new_admin.require_auth()` + `new_admin == PauseKey::PendingAdmin` | `(esc, adm_rot)` | Step 2; promotes and removes `PendingAdmin`. |
| `cancel_admin_transfer` | `(env: Env, admin: Address) -> Result<(), EscrowError>` | `admin.require_auth()` + `admin == PauseKey::Admin` | — | Removes `PendingAdmin`; silently succeeds if none was set. |
| `get_pending_admin` | `(env: Env) -> Option<Address>` | none (view) | — | |
| `create_escrow` | `(env: Env, commission_id: Bytes, client: Address, artist: Address, amount: i128, config_contract: Address) -> Result<(), EscrowError>` | `client.require_auth()` | `(escrow, created)`, `(escrow, created, corr)` | State `∅ → Locked`. **Reads/writes funds:** `token.transfer(client → this contract, amount)` in full. Checks pause, `amount > 0`, non-duplicate id; resolves `get_fee_b` + `get_usdc` from config. `released_amount = 0`. Does **not** verify client balance — `cross_contract::check_sufficient_balance` exists but is never called, so an underfunded client fails via the token contract's own error. Guarded by `with_reentrancy_guard`. |
| `release_payment` | `(env: Env, commission_id: Bytes, config_contract: Address) -> Result<(), EscrowError>` | platform admin (`config_contract.get_adm`) `.require_auth()` | `(escrow, released)` | `Locked → Released`. **Writes funds:** full `r.amount` split — `transfer(this → artist, payout)` and `transfer(this → pw, fee)`. Ignores `released_amount` (safe only because `Locked` excludes `PartiallyReleased`). Guarded. |
| `refund_client` | `(env: Env, commission_id: Bytes, config_contract: Address) -> Result<(), EscrowError>` | platform admin (`get_adm`) `.require_auth()` | `(escrow, refunded)` | `Locked` or `Disputed → Refunded`. **Writes funds:** `transfer(this → client, r.amount)` in full, no fee. `r.amount` is *not* reduced by `released_amount`, but the status gate makes the over-drain unreachable. Pause-guarded. Guarded. |
| `expire_escrow` | `(env: Env, commission_id: Bytes, expiry_ledger: u32) -> Result<(), EscrowError>` | **none** — no `require_auth` anywhere in the body | `(escrow, expired)` | `Locked → Expired` once `env.ledger().sequence() >= expiry_ledger`. Mutates state but **moves no funds**; leaves the USDC sitting in the contract with no later release/refund path reachable (`Refunded` is unreachable once `Expired`). Permissionless by design of the source as written. |
| `open_dispute` | `(env: Env, commission_id: Bytes, initiator: Address) -> Result<(), EscrowError>` | `initiator.require_auth()`, and `initiator` must be `client` or `artist` | `(escrow, disputed)` | `Locked → Disputed`; `DisputeAlreadyOpen` on a second call. **Extends `DataKey::Escrow` TTL** to `get_dispute_ttl_ledgers(env)`. No funds moved. Not re-entrancy guarded. |
| `set_dispute_ttl_ledgers` | `(env: Env, config_contract: Address, ledgers: u32) -> Result<(), EscrowError>` | platform admin (`get_adm`) `.require_auth()` | `(ttl, updated)` | Rejects `0` or `< ESCROW_TTL_LEDGERS` with `InvalidAmount`. Writes `DataKey::DisputeTtlLedgers`; no TTL extension on the key. |
| `get_dispute_ttl_ledgers` | `(env: Env) -> u32` | none (view) | — | Returns configured value or `864_000`. |
| `partial_release` | `(env: Env, commission_id: Bytes, release_amount: i128, config_contract: Address) -> Result<(), EscrowError>` | platform admin (`get_adm`) `.require_auth()` | `(escrow, partial)` | `Locked`/`PartiallyReleased → PartiallyReleased` or `→ Released` when `amount - released_amount` hits 0. **Writes funds:** `transfer(this → artist, payout)` + `transfer(this → pw, fee)` on `release_amount`; `released_amount += release_amount` (checked). Rejects `release_amount <= 0` and `> remaining` with `InvalidAmount`. Re-extends the full `ESCROW_TTL_LEDGERS`. Guarded. |
| `auto_release_on_deadline` | `(env: Env, commission_id: Bytes, auto_release_ledger: u32, config_contract: Address) -> Result<(), EscrowError>` | **none visible** — the readable body (lib.rs:562–598) contains no `require_auth` and never resolves `get_adm`; `unknown` whether the truncated tail adds a check | `(escrow, autorls)` — **topics/payload opening written at lib.rs:650–652, call never closed** | `Locked`/`PartiallyReleased`; requires `sequence() >= auto_release_ledger` else `NotExpired`, and `remaining > 0` else `InvalidAmount`. Sets `released_amount = amount` and `status = Released` (lib.rs:597–598) then **stops mid-`EFFECTS`**. Visible: fee split on `remaining` and resolution of `get_usdc`/`get_pw`. Missing/unparseable: the `save_escrow`, any TTL extension, the artist/fee transfers, the `autorls` event's closing `);`, the closure `})` and the function's `}`. Because this is a full-amount payout to the artist with no fee-authorizing signature, the missing tail is the security-critical gap. |
| `cancel_escrow` | `(env: Env, commission_id: Bytes, config_contract: Address, artist_amount: i128, client_refund: i128) -> Result<(), EscrowError>` — **header nested inside another function's closure; not a valid export** | platform admin (`get_adm`) `.require_auth()` (lib.rs:629–630) | `(escrow, cancelled)` (lib.rs:664–667) | `Locked` or `Disputed → Cancelled`. **Writes funds:** `transfer(this → artist, payout)`, `transfer(this → pw, fee)` (both unconditional), `transfer(this → client, client_refund)` if `> 0`. Requires `artist_amount + client_refund == r.amount` (`InvalidSplit` otherwise) and both `>= 0` (`InvalidAmount`). Fee charged only on the artist's share. Tests confirm the escrow is drained to exactly 0. Body as written at lib.rs:637–669 is spliced with `auto_release_on_deadline`'s locals. |
| `get_escrow` | `(env: Env, commission_id: Bytes) -> Result<EscrowRecord, EscrowError>` | none (view) | — | `NotFound` if the key is absent. |
| `begin_atomic_commit` | `(env: Env, commission_id: Bytes) -> Result<AtomicCommitMarker, EscrowError>` | none — no `require_auth` | — | Requires escrow to exist and be `Locked` or `Disputed`; `AlreadyExists` on a duplicate marker. Creates `InProgress` marker (`participants = 2`, `confirmed = 0`). No funds moved. |
| `confirm_atomic_step` | `(env: Env, commission_id: Bytes, from: Address) -> Result<u32, EscrowError>` | `from.require_auth()` (cross_contract.rs:206) | — | Increments `confirmed`; `AtomicCommitStateInvalid` once `confirmed >= participants`. Any address may sign; the source does not bind `from` to client/artist. |
| `finalize_atomic_commit` | `(env: Env, commission_id: Bytes) -> Result<AtomicCommitMarker, EscrowError>` | none — no `require_auth` | — | Requires `InProgress` + `confirmed >= participants` (`AtomicCommitNotReady` otherwise). Sets escrow `→ Released` and marker `→ Settled` **without transferring any funds**; after this the escrow can no longer be refunded, so the held USDC remains in the contract until `atomic_escrow_to_commission` moves it. |
| `rollback_atomic_commit` | `(env: Env, commission_id: Bytes) -> Result<AtomicCommitMarker, EscrowError>` | none — no `require_auth` | `(escrow, rollback, corr)` | Legal only from `InProgress` or `Failed`; sets `→ RolledBack`, leaves the escrow record untouched. |
| `get_atomic_commit` | `(env: Env, commission_id: Bytes) -> Result<AtomicCommitMarker, EscrowError>` | none (view) | — | |
| `verify_agreement_consistency` | `(env: Env, commission_contract: Address, commission_id: Bytes, expected_amount: i128) -> Result<bool, EscrowError>` | none — cross-contract read | — | Calls `get_agreement_escrow_amount` on the commission contract and compares to `expected_amount`. |
| `atomic_escrow_to_commission` | `(env: Env, commission_id: Bytes, config_contract: Address, commission_contract: Address) -> Result<AtomicCommitMarker, EscrowError>` | platform admin (`get_adm`) `.require_auth()` | `(escrow, released)`, `(escrow, settled, corr)` | `Locked`/`Disputed → Released`. **Writes funds:** `transfer(this → pw, fee)` if `> 0` and `transfer(this → commission_contract, payout)` if `> 0`, on the **full** `record.amount`. On consistency mismatch it writes a permanent `Failed` marker and returns `Ok` with **no funds moved** (verified by test). Guarded. |
| `health_check` | `(env: Env) -> shared::health::HealthReport` | none (view-ish: **writes**) | `(hlth_alrt)` via `shared` when an alert fires | Reads `HealthKey::Metrics`, classifies, and calls `maybe_auto_rollback` when `report.anomaly` — which mutates rollout state and can set the `shared::pause` paused flag. |
| `get_health_metrics` | `(env: Env) -> shared::health::HealthMetrics` | none (view) | — | |
| `get_sla_targets` | `(env: Env) -> shared::health::SlaTargets` | none (view) | — | `env` explicitly discarded. |
| `set_alert_config` | `(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `(hlth_alrt)` may fire later | `admin` is **not** compared against `PauseKey::Admin` or the config admin. |
| `get_alert_config` | `(env: Env) -> shared::health::AlertConfig` | none (view) | — | |
| `detect_anomaly` | `(env: Env) -> bool` | none (view) | — | |
| `report_ok` | `(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments ok counters / refreshes `last_ok_ledger`. |
| `report_error` | `(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments error counters; feeds the rollback trigger. |
| `set_feature_flag` | `(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` | `(feat_flg)` | `admin` not checked against any stored admin. |
| `is_feature_enabled` | `(env: Env, flag: Symbol) -> bool` | none (view) | — | Always `false` once the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `(canary)` | Panics via `shared` if `canary_bps > 10_000`. |
| `route_to_canary` | `(env: Env, caller: Address) -> bool` | none (view) | — | Sticky SHA-256 bucket comparison against `canary_bps`. |
| `get_rollout_state` | `(env: Env) -> shared::rollout::RolloutState` | none (view) | — | |
| `set_rollback_trigger` | `(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `(rb_trig)` | Panics via `shared` on `0` or `> 10_000`. |
| `should_rollback` | `(env: Env) -> bool` | none (view) | — | |
| `trigger_rollback` | `(env: Env, admin: Address)` | `admin.require_auth()` | `(rollback)`, `(contract_paused)` | Zeroes canary traffic, disables all flags, sets `shared::pause::PauseDataKey::Paused = true`. That key is never read by `require_not_paused`, so **rollback does not actually stop `create_escrow`/`refund_client`**. |

**Events.**

All `env.events().publish(...)` shapes reachable from escrow, deduplicated across repeat call sites.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `(esc, paused)` | `symbol_short!("esc")`, `symbol_short!("paused")` | `admin: Address` | `pause` (lib.rs:154) |
| `(esc, unpaused)` | `esc`, `unpaused` | `admin: Address` | `unpause` (lib.rs:172) |
| `(esc, adm_prop)` | `esc`, `adm_prop` | `(admin, new_admin): (Address, Address)` | `transfer_admin` (lib.rs:205) |
| `(esc, adm_rot)` | `esc`, `adm_rot` | `(previous, new_admin): (Address, Address)` | `accept_admin` (lib.rs:232) |
| `(escrow, created)` | `escrow`, `created` | `(commission_id, amount): (Bytes, i128)` | `create_escrow` (lib.rs:306) |
| `(escrow, released)` | `escrow`, `released` | `(commission_id, payout, fee): (Bytes, i128, i128)` | `release_payment` (lib.rs:346) **and** `atomic_escrow_to_commission` (cross_contract.rs:363) — same shape, two call sites |
| `(escrow, partial)` | `escrow`, `partial` | `(commission_id, release_amount, payout, fee, released_amount)` | `partial_release` (lib.rs:546) |
| `(escrow, refunded)` | `escrow`, `refunded` | `(commission_id, client, amount): (Bytes, Address, i128)` | `refund_client` (lib.rs:384) |
| `(escrow, expired)` | `escrow`, `expired` | `(commission_id, expiry_ledger): (Bytes, u32)` | `expire_escrow` (lib.rs:404) |
| `(escrow, disputed)` | `escrow`, `disputed` | `(commission_id, initiator): (Bytes, Address)` | `open_dispute` (lib.rs:428) |
| `(escrow, cancelled)` | `escrow`, `cancelled` | `(commission_id, payout, fee, client_refund)` | `cancel_escrow` (lib.rs:664) |
| `(escrow, autorls)` — **truncated** | `escrow`, `autorls` (written) | opening written as `(commission_id, auto_release_ledger, remaining, payout, fee)`; the `publish(` call opened at lib.rs:650 is **never closed**, so the exact payload type is `unknown` | `auto_release_on_deadline` — the `publish(` sits in the spliced region; which function it truly belongs to is not determinable from the file |
| `(ttl, updated)` | `symbol_short!("ttl")`, `symbol_short!("updated")` | `ledgers: u32` | `set_dispute_ttl_ledgers` (lib.rs:459) |
| `(escrow, created, corr)` | `Symbol::new(env, "escrow")`, `created`, `Symbol::new(env, "corr")` | `(correlation_id: BytesN<32>, key: Bytes)` | `create_escrow` via `correlation_publish` (lib.rs:310 → `shared::correlation::publish`) |
| `(escrow, rollback, corr)` | `"escrow"`, `rollback`, `"corr"` | `(id, key)` | `rollback_atomic_commit` via `correlation_publish` (cross_contract.rs:278) |
| `(escrow, settled, corr)` | `"escrow"`, `settled`, `"corr"` | `(id, key)` | `atomic_escrow_to_commission` via `correlation_publish` (cross_contract.rs:367) |
| `(hlth_alrt)` | `hlth_alrt` (single topic) | `(status, error_bps, stalled)` | `shared::health` — only when an alert config is set and the cooldown has elapsed; reachable from `health_check` |
| `(canary)` | `canary` | `(canary, stable, canary_bps)` | `shared::rollout::set_canary_deployment` ← `set_canary_deployment` |
| `(feat_flg)` | `feat_flg` | `(flag: Symbol, enabled: bool)` | `shared::rollout::set_feature_flag` ← `set_feature_flag` |
| `(rb_trig)` | `rb_trig` | `error_bps: u32` | `shared::rollout::set_rollback_trigger` ← `set_rollback_trigger` |
| `(rollback)` | `rollback` | `ledger sequence: u32` | `shared::rollout::apply_rollback` ← `maybe_auto_rollback` (from `health_check`) and `trigger_rollback` |
| `(contract_paused)` | `Symbol::new(env, "contract_paused")` | `ContractPausedEvent { admin: Address }` | `shared::rollout::trigger_rollback` ← `trigger_rollback` |

**Errors.**

`EscrowError` in `errors.rs` is the crate's only `#[contracterror]` enum. `get_suggestion` maps each variant to a short symbol; there is no arbiter role anywhere in the state machine.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyExists` | 1 | `initialize` on an already-initialized contract; `create_escrow` on a duplicate `commission_id`; `begin_atomic_commit` on an existing marker |
| `NotFound` | 2 | `get_escrow` / `get_atomic_commit` on an absent key; `begin_atomic_commit` / `confirm_atomic_step` / `atomic_escrow_to_commission` on an absent escrow |
| `InvalidStatus` | 3 | Operation not legal from the record's current `CommissionStatus` (release outside `Locked`, refund outside `Locked`/`Disputed`, partial release / auto-release outside `Locked`/`PartiallyReleased`, cancel outside `Locked`/`Disputed`, dispute outside `Locked`, expire outside `Locked`, atomic commit outside `Locked`/`Disputed`) |
| `Unauthorized` | 4 | `admin` argument ≠ `PauseKey::Admin` on `pause`/`unpause`/`transfer_admin`/`cancel_admin_transfer`; missing `PendingAdmin` on `accept_admin`; `open_dispute` initiator is neither client nor artist |
| `InvalidAmount` | 5 | `create_escrow` with `amount <= 0`; `partial_release` with `release_amount <= 0` or `> remaining`; `auto_release_on_deadline` with `remaining <= 0`; `cancel_escrow` with a negative `artist_amount` or `client_refund`; `set_dispute_ttl_ledgers` with `0` or `< ESCROW_TTL_LEDGERS` |
| `InvalidFeeBps` | 6 | Declared but **never constructed** anywhere in the crate — `fee_bps` is read from `PlatformConfig` and not range-checked here |
| `DisputeAlreadyOpen` | 7 | `open_dispute` on an already-`Disputed` escrow |
| `NotExpired` | 8 | `expire_escrow` or `auto_release_on_deadline` called before `env.ledger().sequence() >= expiry_ledger` / `auto_release_ledger` |
| `Reentrant` | 9 | `with_reentrancy_guard` entered while `DataKey::ReentrancyLock` is set |
| `InvalidAddress` | 10 | Declared but **never constructed** — `create_escrow` does not check that `client != artist` despite the `Display` text |
| `InsufficientBalance` | 11 | Constructed only inside `cross_contract::check_sufficient_balance`, which is a `pub fn` **never called by the contract**; also raised via `panic_with_error!` (host panic, not a `Result::Err`) |
| `ArithmeticOverflow` | 12 | `checked_mul`/`checked_add`/`checked_sub` failure in `calculate_fee_split`, `partial_release` (`remaining` and `released_amount`), `cancel_escrow`'s split sum, `auto_release_on_deadline`'s `remaining`, or `confirm_atomic_step`'s counter |
| `InvalidSplit` | 13 | `cancel_escrow` where `artist_amount + client_refund != r.amount` |
| `ContractPaused` | 14 | `require_not_paused` when `PauseKey::Paused` is `true` — blocks `create_escrow` and `refund_client` only |
| `AtomicCommitNotReady` | 15 | `finalize_atomic_commit` while `confirmed < participants` |
| `AtomicCommitStateInvalid` | 16 | `confirm_atomic_step` past `participants`, or `require_in_progress` failing in `confirm`/`finalize`; `rollback_atomic_commit` from `Settled` |
| `CrossContractConsistencyFailed` | 17 | Declared for the atomic flow (#656) but **never constructed** — a consistency mismatch in `atomic_escrow_to_commission` records a `Failed` marker and returns `Ok(marker)` instead of this error |

**Storage.** See [STORAGE.md](./STORAGE.md#escrow).

**Compile status.** Does not compile. `cargo build -p escrow` reports exactly two errors, both parse-level, both in `contracts/escrow/src/lib.rs`:  1. `error: mismatched closing delimiter: }` at `lib.rs:650:33` — the `env.events().publish(` at `lib.rs:650:9` is unclosed; the `}` at `lib.rs:669` is flagged mismatched, with rustc pointing at the `storage::with_reentrancy_guard(&env, || {` opened at `lib.rs:637` as the delimiter it was "possibly meant for". 2. `error: this file contains an unclosed delimiter` at `lib.rs:845:24` (`mod correlation_tests;`) — the unclosed delimiters are `impl EscrowContract {` (lib.rs:99), the `pub fn auto_release_on_deadline(...) -> Result<(), EscrowError> {` signature (ending lib.rs:567), and the closure `storage::with_reentrancy_guard(&env, || {` (lib.rs:593); the `}` at `lib.rs:825` absorbs all three with mismatched indentation.  Root cause: the file is spliced at `lib.rs:598–599`. `auto_release_on_deadline`'s body stops after `r.status = CommissionStatus::Released;` with no `save_escrow`, no TTL extension, no transfers, no closing `})` or `}`, and the doc comment + `pub fn cancel_escrow(...)` header of the following function (lib.rs:599–613) begin inside that unclosed closure. The region `lib.rs:637–669` mixes the two functions: it opens a second `with_reentrancy_guard` closure, uses `auto_release_ledger` and `remaining` (locals of `auto_release_on_deadline`) inside the `autorls` event, and also references `client_refund`/`client` (from `cancel_escrow`), which cannot be in scope at that nesting.  Consequently unreachable / uncompilable as written: `auto_release_on_deadline` (truncated — its `save_escrow`, TTL call, artist and platform-wallet transfers, completed `autorls` event and closers are absent from the file and were not reconstructed), `cancel_escrow` (header nested in a closure body, body spliced), the 24 `pub fn`s from `get_escrow` (lib.rs:672) to `trigger_rollback` (lib.rs:821), and all 8 `#[cfg(test)]` module declarations (lib.rs:828–845). The 19 `pub fn`s from `initialize` (lib.rs:104) through `partial_release` (lib.rs:484) are the only lexically valid part of the contract block. The failure is parse-level, so no name resolution, macro expansion of `#[contractimpl]`, or type checking runs — `errors.rs`, `storage.rs` and `cross_contract.rs` are individually well-formed and are documented from source as written, but the crate as a whole produces no WASM. No fix was attempted.


### licensing

> This crate has no `//!` module doc comment. Nearest header comment (contracts/licensing/src/lib.rs:1-9): "Implements Creative Licensing Marketplace — closes #616. Acceptance Criteria: Support sub-licensing rights; Implement usage rights marketplace; Add usage tracking and auditing; Support license derivatives; Implement dispute resolution for usage."

Source: `contracts/licensing/`

**Purpose.**

An on-chain registry for licensing digital creative works. A rights holder issues a `LicenseRecord` scoped as `Personal`, `Commercial`, `Exclusive`, or `SubLicense`, optionally sub-licensable and optionally expiring at a ledger. A licensee can derive further sub-licenses from any active, sub-licensable parent — the parent licensee becomes the `owner` of the child and sub-licensing stops there (`sub_licensable: false`). Every use must be logged by the licensee via `record_usage`, producing an auditable, paginated `UsageEntry` trail that is rejected once the license is revoked, expired, or disputed. Disputes are raised by any complainant (which flags the license `Disputed` and thereby halts usage logging) and resolved solely by the stored admin, who either revokes the license (`UpheldForLicensor`) or reactivates it.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` (declared but never referenced in `src/lib.rs` — no `shared::` path appears in the source). Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to 21.7.7 in `Cargo.lock`); dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address)` | `admin.require_auth()` | `init` | Writes `Admin`, `LicenseCount = 0`, `DisputeCount = 0`. Not idempotent-guarded — a second call overwrites the admin. |
| `create_license` | `fn create_license(env: Env, owner: Address, licensee: Address, asset_id: String, license_type: LicenseType, price: i128, sub_licensable: bool, expires_at: u32) -> u64` | `owner.require_auth()` | `licensed` | Issues a primary license with `parent_id = 0`, `status = Active`, `created_at = env.ledger().sequence()`, and `sub_licensable` as supplied. Also seeds `UsageCount(id) = 0`. `price` is recorded only — no payment is taken or escrowed. The `Exclusive` variant carries no enforcement: nothing prevents other licenses being issued over the same `asset_id`. Returns the new id. |
| `create_sub_license` | `fn create_sub_license(env: Env, parent_id: u64, sub_licensee: Address, price: i128, expires_at: u32) -> u64` | `parent.licensee.require_auth()` — the current licensee of the parent, not the caller argument | `sublicens` | Loads the parent (`expect("parent license not found")`), then requires `status == Active` and `sub_licensable == true`. The child inherits `asset_id`, takes `owner = parent.licensee`, `license_type = SubLicense`, `parent_id`, and hard-codes `sub_licensable: false` (so sub-licenses cannot be sub-licensed further). Does not check the parent's `expires_at`. Returns the new id. |
| `revoke_license` | `fn revoke_license(env: Env, caller: Address, license_id: u64)` | `caller.require_auth()`, plus `record.owner == caller` | `revoked` | Owner-only — the admin **cannot** revoke. Sets `status = Revoked` unconditionally; a revoked license can be re-`expire`d but never reactivated, since `resolve_dispute` is the only path that sets `Active` and requires a `Pending` dispute. |
| `expire_license` | `fn expire_license(env: Env, caller: Address, license_id: u64)` | `caller.require_auth()`, plus `caller == admin \|\| caller == record.owner` | `expired` | Sets `status = Expired`. Unlike revocation, the admin is allowed. |
| `record_usage` | `fn record_usage(env: Env, user: Address, license_id: u64, context: String)` | `user.require_auth()`, plus `record.licensee == user` | `usage` | Requires `status == Active` — so usage is blocked while the license is `Revoked`, `Expired`, or `Disputed`. Enforces expiry only when `expires_at > 0`, asserting `env.ledger().sequence() <= expires_at`. Appends a `UsageEntry` at index `count` and increments `UsageCount`. |
| `get_usage_count` | `fn get_usage_count(env: Env, license_id: u64) -> u64` | none (view) | — | Returns `UsageCount(license_id)`, defaulting to `0`. |
| `raise_dispute` | `fn raise_dispute(env: Env, complainant: Address, license_id: u64, description: String) -> u64` | `complainant.require_auth()` | `dispute` | Any address may complain; there is no requirement to be owner or licensee. First flips the license to `LicenseStatus::Disputed` (freezing `record_usage`), then creates a `LicenseDispute` with `outcome = DisputeOutcome::Pending` and `raised_at = env.ledger().sequence()`. The second complaint on an already-`Disputed` license still succeeds. Returns the dispute id. |
| `resolve_dispute` | `fn resolve_dispute(env: Env, admin: Address, dispute_id: u64, outcome: DisputeOutcome)` | `admin.require_auth()`, plus `admin == stored_admin` | `resolved` | Requires `outcome == Pending`, else panics. Sets the dispute's `outcome`, then maps the license status: `UpheldForLicensor → Revoked`, and every other outcome (including `Pending` and `Settled`) → `Active`. |
| `get_license` | `fn get_license(env: Env, license_id: u64) -> LicenseRecord` | none (view) | — | Loads `License(id)`; panics `"license not found"` if absent. |
| `get_dispute` | `fn get_dispute(env: Env, dispute_id: u64) -> LicenseDispute` | none (view) | — | Loads `Dispute(dispute_id)`; panics `"dispute not found"` if absent. |
| `get_usage_entries` | `fn get_usage_entries(env: Env, license_id: u64, offset: u64, limit: u64) -> Vec<UsageEntry>` | none (view) | — | Paginated slice bounded by `(offset + limit).min(UsageCount(license_id))`. |

Internal (not part of the external interface): `next_license_id`, `next_dispute_id`.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `admin` | `initialize` |
| `licensed` | `[contract, "licensed", asset_id]` | `(id, owner, licensee, license_type)` | `create_license` |
| `sublicens` | `[contract, "sublicens", parent.asset_id]` | `(id, parent_id, sub_licensee)` | `create_sub_license` |
| `revoked` | `[contract, "revoked"]` | `license_id` | `revoke_license` |
| `expired` | `[contract, "expired"]` | `license_id` | `expire_license` |
| `usage` | `[contract, "usage", license_id]` | `(user, context)` | `record_usage` |
| `dispute` | `[contract, "dispute", license_id]` | `(dispute_id, complainant, description)` | `raise_dispute` |
| `resolved` | `[contract, "resolved"]` | `(dispute_id, outcome)` | `resolve_dispute` |

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| no `#[contracterror]` enum — all failures are host panics | n/a | Every rejection is a string-literal `assert!` or an `Option::expect`. In `initialize`: none. In `create_license`: none. In `create_sub_license`: `expect("parent license not found")`, `"parent license is not active"`, `"parent license does not allow sub-licensing"`. In `revoke_license`: `expect("license not found")`, `"only the license owner can revoke"`. In `expire_license`: `expect("contract not initialized")`, `expect("license not found")`, `"unauthorized"`. In `record_usage`: `expect("license not found")`, `"license is not active"`, `"caller is not the licensee"`, `"license has expired"`. In `raise_dispute`: `expect("license not found")`. In `resolve_dispute`: `expect("contract not initialized")`, `expect("dispute not found")`, `"only admin can resolve disputes"`, `"dispute already resolved"`, `expect("license not found")`. In the view functions: `expect("license not found")`, `expect("dispute not found")`. |

**Storage.** See [STORAGE.md](./STORAGE.md#licensing).

**Compile status.** `compiles` — `cargo build -p licensing` finishes cleanly (no warnings emitted).


### mentorship

> This crate has no `//!` module doc comment. Nearest header comment (contracts/mentorship/src/lib.rs:1-9): "Implements Mentorship Program Contract — closes #613. Acceptance Criteria: Support mentor-mentee pairing; Track mentorship milestones; Implement feedback collection; Add certification upon completion; Support compensation for mentors."

Source: `contracts/mentorship/`

**Purpose.**

On-chain bookkeeping for mentor–mentee engagements, aimed at emerging creative talent. A mentor proposes an engagement with an ordered milestone list and an agreed `compensation` plus a token address; the mentee must accept before work starts. Milestones move `Pending → Submitted → Approved`/`Rejected`, and when the mentor approves the last outstanding milestone the engagement auto-completes and the compensation is transferred to the mentor. After an engagement ends (completed or cancelled) both parties may leave 1–5 star feedback, and either party may trigger issuance of a completion certificate — which is purely an on-chain flag and event for off-chain services to react to. This contract is not an escrow: no funds are taken at proposal time, so the completion transfer can only succeed if the contract itself already holds the compensation balance.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` (declared but never referenced in `src/lib.rs` — no `shared::` path appears in the source). Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to 21.7.7 in `Cargo.lock`); dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address)` | `admin.require_auth()` | `init` | Writes `Admin` and `EngagementCount = 0`. Not idempotent-guarded — a second call overwrites the admin. |
| `propose_engagement` | `fn propose_engagement(env: Env, mentor: Address, mentee: Address, description: String, milestones: Vec<(String, String)>, compensation: i128, token: Address) -> u64` | `mentor.require_auth()` | `proposed` | Asserts `compensation >= 0` and that `milestones` is non-empty. Creates the record in `EngagementStatus::Proposed` with `completed_at = 0` and `certificate_issued = false`, and materialises the milestone vector with sequential `index` values, `status = Pending`, `approved_at = 0`. No funds are moved or escrowed. Returns the new id. |
| `accept_engagement` | `fn accept_engagement(env: Env, mentee: Address, engagement_id: u64)` | `mentee.require_auth()`, plus `engagement.mentee == mentee` | `accepted` | Requires `status == Proposed`; flips to `Active`. This is the only path out of `Proposed` other than `cancel_engagement`. |
| `cancel_engagement` | `fn cancel_engagement(env: Env, caller: Address, engagement_id: u64)` | `caller.require_auth()`, plus `caller == engagement.mentor \|\| caller == engagement.mentee` | `cancelled` | Allowed from `Proposed` or `Active` only; sets `status = Cancelled`. Cancelling an `Active` engagement releases any pending compensation obligation without payment. |
| `submit_milestone` | `fn submit_milestone(env: Env, mentee: Address, engagement_id: u64, milestone_index: u32)` | `mentee.require_auth()`, plus `engagement.mentee == mentee` | `ms_sub` | Requires `status == Active` and `milestones.get(index)` to exist and be `Pending`. Sets the entry's `status` to `Submitted` via `Vec::set` with `..ms` struct-update, leaving `approved_at` untouched. A `Rejected` milestone cannot be resubmitted. |
| `review_milestone` | `fn review_milestone(env: Env, mentor: Address, engagement_id: u64, milestone_index: u32, approve: bool)` | `mentor.require_auth()`, plus `engagement.mentor == mentor` | `ms_rev`, and `complete` when the final milestone is approved | Requires `status == Active` and the milestone to be `Submitted`. Sets `status` to `Approved` (with `approved_at = env.ledger().sequence()`) or `Rejected` (`approved_at = 0`). If `approve` and every milestone is `Approved`, auto-completes: sets `status = Completed`, `completed_at = now`, and, when `compensation > 0`, issues `token::Client::new(&env, &engagement.token).transfer(&env.current_contract_address(), &engagement.mentor, &engagement.compensation)` — i.e. the contract pays out of its own balance, which is never funded by this contract. |
| `submit_feedback` | `fn submit_feedback(env: Env, author: Address, engagement_id: u64, rating: u32, comment: String)` | `author.require_auth()`, plus `author == mentor \|\| author == mentee` | `feedback` | Asserts `rating` is within 1..=5. Requires `status == Completed \|\| Cancelled`. Any number of entries may be submitted per engagement, and the same author may submit repeatedly. Appends a `FeedbackEntry` at index `count` and increments `FeedbackCount`. |
| `issue_certificate` | `fn issue_certificate(env: Env, caller: Address, engagement_id: u64)` | `caller.require_auth()`, plus `caller == admin \|\| caller == mentor \|\| caller == mentee` | `certified` | Requires `status == Completed` and `!certificate_issued`. Only sets `certificate_issued = true` — no NFT is minted in-contract; the event is intended for off-chain services. |
| `get_engagement` | `fn get_engagement(env: Env, engagement_id: u64) -> MentoringEngagement` | none (view) | — | Loads `Engagement(id)`; panics `"engagement not found"` if absent. |
| `get_milestones` | `fn get_milestones(env: Env, engagement_id: u64) -> Vec<MentoringMilestone>` | none (view) | — | Loads the whole `Milestones(id)` vector; panics `"milestones not found"` if absent. |
| `get_feedback` | `fn get_feedback(env: Env, engagement_id: u64) -> Vec<FeedbackEntry>` | none (view) | — | Returns all entries from index `0` up to `FeedbackCount(engagement_id)`; empty for an unknown engagement. |

Internal (not part of the external interface): `next_id`, `load_engagement`.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `admin` | `initialize` |
| `proposed` | `[contract, "proposed"]` | `(id, mentor, mentee)` | `propose_engagement` |
| `accepted` | `[contract, "accepted"]` | `engagement_id` | `accept_engagement` |
| `cancelled` | `[contract, "cancelled"]` | `(engagement_id, caller)` | `cancel_engagement` |
| `ms_sub` | `[contract, "ms_sub", engagement_id]` | `milestone_index` | `submit_milestone` |
| `ms_rev` | `[contract, "ms_rev", engagement_id]` | `(milestone_index, approve)` | `review_milestone` |
| `complete` | `[contract, "complete"]` | `engagement_id` | `review_milestone` (only when `approve` and all milestones are `Approved`) |
| `feedback` | `[contract, "feedback", engagement_id]` | `(author, rating, comment)` | `submit_feedback` |
| `certified` | `[contract, "certified"]` | `(engagement_id, mentee, mentor)` | `issue_certificate` |

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| no `#[contracterror]` enum — all failures are host panics | n/a | Every rejection is a string-literal `assert!` or an `Option::expect`. In `propose_engagement`: `"compensation must be non-negative"`, `"at least one milestone required"`. In `accept_engagement`: `expect("engagement not found")`, `"caller is not the mentee"`, `"engagement is not in Proposed state"`. In `cancel_engagement`: `expect("engagement not found")`, `"only mentor or mentee can cancel"`, `"cannot cancel a completed or already-cancelled engagement"`. In `submit_milestone`: `expect("engagement not found")`, `"caller is not the mentee"`, `"engagement is not active"`, `expect("milestones not found")`, `expect("milestone index out of range")`, `"milestone already submitted or approved"`. In `review_milestone`: `expect("engagement not found")`, `"caller is not the mentor"`, `"engagement is not active"`, `expect("milestones not found")`, `expect("milestone index out of range")`, `"milestone must be in Submitted state"`. In `submit_feedback`: `"rating must be between 1 and 5"`, `expect("engagement not found")`, `"only mentor or mentee may submit feedback"`, `"feedback can only be submitted after the engagement ends"`. In `issue_certificate`: `expect("engagement not found")`, `expect("contract not initialized")`, `"unauthorized"`, `"engagement must be completed to issue a certificate"`, `"certificate already issued"`. In the view functions: `expect("engagement not found")`, `expect("milestones not found")`. The auto-complete `token::Client::transfer` in `review_milestone` is not wrapped, so an underfunded contract surfaces as a raw host error rather than a string panic. |

**Storage.** See [STORAGE.md](./STORAGE.md#mentorship).

**Compile status.** `compiles` — `cargo build -p mentorship` finishes cleanly (no warnings emitted).


### messaging

> Messaging contract — artist-client messaging with encryption metadata support. Closes #596.

Source: `contracts/messaging/`

**Purpose.**

`messaging` is a two-party conversation store for artists and their clients. Encryption is deliberately *not* performed on-chain: the contract stores the ciphertext (or plaintext) `body` verbatim and never inspects it, so a client encrypts before calling `send_message`. On top of the message log it layers the state a chat UI needs — per-conversation read receipts, ephemeral typing indicators, and soft deletion (the body is zeroed and a `deleted` flag set, so history is preserved but the content is not) — and it throttles each sender to one message per `RATE_LIMIT_LEDGERS` per conversation to stop spam. History is read back through a bounded, 1-based sequence-number page (`get_messages`) rather than an unbounded listing. The `admin` written at `initialize` is stored but is **not** consulted by any messaging entry point — all authorization is participant-scoped; the admin surface is only the delegated `shared` health/rollout block.

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — used for the health/rollout surface. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with the `testutils` feature as a dev-dependency. Features: `testutils = ["soroban-sdk/testutils"]` (no `default` key). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. `semver_types.rs` is not a dependency: it is `include!`d from `contracts/semver_types.rs` at `lib.rs:28`.

**Public interface.**

29 externally callable `pub fn`s. Internal `fn`s — none in this crate's `lib.rs` besides the `#[contractimpl]` boundary, since everything is inlined in the block. The three `get_version` / `get_version_metadata` / `is_version_compatible` rows are generated by the `impl_semver_queries!()` macro invocation at `lib.rs:49` (defined in `contracts/semver_types.rs`), not hand-written.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), MessagingError>` | `admin.require_auth()` | — | Guards on `Initialized`, not on `Admin`. Single-shot. Does not touch `Admin` on the failure path |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | Macro-generated; parses `CARGO_PKG_VERSION` |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Macro-generated; returns crate name, semver, min-compatible, `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated; `major` must match, and under `major == 0` the `minor` must match with `patch >=` |
| `create_conversation` | `create_conversation(env: Env, conv_id: Bytes, participant_a: Address, participant_b: Address) -> Result<(), MessagingError>` | `participant_a.require_auth()` | `conv_new` | Only `participant_a` signs — `participant_b` is recorded **without consenting**, so a caller can open a conversation naming anyone. `conv_id` is caller-chosen; the doc suggests `SHA256(sort([a, b]))` but nothing enforces it. `participant_a == participant_b` is not rejected |
| `get_conversation` | `get_conversation(env: Env, conv_id: Bytes) -> Result<Conversation, MessagingError>` | none (view) | — | No participant check — any caller can read any conversation record |
| `send_message` | `send_message(env: Env, conv_id: Bytes, sender: Address, body: String) -> Result<u32, MessagingError>` | `sender.require_auth()` | `msg_sent` | Returns the new 1-based `seq`. Enforces `MAX_MESSAGE_LEN`, sender-is-a-participant, and the `RATE_LIMIT_LEDGERS` window. `message_id` is built as `conv_id` ++ 4 big-endian `seq` bytes — it is **not** a hash and does not include the sender, contradicting the field doc ("hash of conv_id + sender + seq") and the `Message` doc |
| `get_messages` | `get_messages(env: Env, conv_id: Bytes, caller: Address, from_seq: u32, limit: u32) -> Result<Vec<Message>, MessagingError>` | **no `require_auth` — `none`**; participant identity is checked but never signed | — | Returns `[from_seq, from_seq + min(limit, 100))`, clamped to `message_count + 1`. Because `caller` is unauthenticated, anyone can read any conversation's messages by passing a participant's address — the participant check is not a security boundary here |
| `delete_message` | `delete_message(env: Env, conv_id: Bytes, seq: u32, caller: Address) -> Result<(), MessagingError>` | `caller.require_auth()` | `msg_del` | Sender-only (`CannotDeleteOthers`), refuses an already-deleted message (`AlreadyDeleted`), zeroes `body` and sets `deleted = true` |
| `mark_read` | `mark_read(env: Env, conv_id: Bytes, reader: Address, up_to_seq: u32) -> Result<(), MessagingError>` | `reader.require_auth()` | `msg_read` | Participant-only. **No validation that `up_to_seq <= message_count`**, and no monotonicity check — a smaller value overwrites a larger watermark, so the receipt can regress |
| `get_read_receipt` | `get_read_receipt(env: Env, conv_id: Bytes, reader: Address) -> Option<ReadReceipt>` | none (view) | — | Returns `None` when never marked; no participant check |
| `set_typing` | `set_typing(env: Env, conv_id: Bytes, typer: Address) -> Result<(), MessagingError>` | `typer.require_auth()` | `typing` | Participant-only. Overwrites `set_ledger`; the 30-ledger expiry is documented but not enforced here |
| `get_typing` | `get_typing(env: Env, conv_id: Bytes, typer: Address) -> Option<TypingIndicator>` | none (view) | — | Applies the local `TYPING_EXPIRY_LEDGERS = 30` filter; `None` when absent or stale |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt` (conditional), `rollback` (conditional) | Classifies health, then calls `shared::rollout::maybe_auto_rollback` when `report.anomaly` |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | Includes the live `paused` flag |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | `env` explicitly discarded |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` (on the **parameter**) | `alrt_cfg` | The parameter is authorized but **not compared to the stored `DataKey::Admin`** — any address can overwrite the alert config. Panics inside `shared` on bps > 10000, degraded > unhealthy, or zero stall/cooldown |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to `default_alert_config()` |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | Read-only |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | — | Same unchecked-parameter gap as `set_alert_config` |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | — | Same unchecked-parameter gap; feeds `should_rollback` |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` (on the parameter) | `feat_flg` | Same unchecked-parameter gap; appends to `FlagIndex` if absent |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | Forced `false` while phase is `RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` (on the parameter) | `canary` | Same unchecked-parameter gap; panics above 10,000 bps |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | **no `require_auth` — `none`** | — | `caller` is unauthenticated, so any caller can ask the sticky-split question for any address; read-only, so the impact is informational |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | Aggregates phase, bps, canary/stable, rollback bps, flag count |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` (on the parameter) | `rb_trig` | Same unchecked-parameter gap; panics on `0` or `> 10_000` |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | `true` once phase is `RolledBack`, else error-rate vs trigger |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | `rollback`, `contract_paused` | Zeroes `CanaryBps`, sets `RolledBack`, disables all indexed flags, sets `Paused = true` |

**Events.**

Five contract-specific shapes plus seven reached through the delegated `shared` health/rollout entry points.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `conv_new` | `(symbol_short!("conv_new"),)` | `(conv_id: Bytes, participant_a: Address, participant_b: Address)` | `create_conversation` |
| `msg_sent` | `(symbol_short!("msg_sent"),)` | `(conv_id: Bytes, sender: Address, seq: u32)` | `send_message` |
| `msg_del` | `(symbol_short!("msg_del"),)` | `(conv_id: Bytes, seq: u32, caller: Address)` | `delete_message` |
| `msg_read` | `(symbol_short!("msg_read"),)` | `(conv_id: Bytes, reader: Address, up_to_seq: u32)` | `mark_read` |
| `typing` | `(symbol_short!("typing"),)` | `(conv_id: Bytes, typer: Address)` | `set_typing` |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check`, via `shared::health` — suppressed inside the cooldown window |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `(unhealthy_error_bps: u32)` | `set_alert_config`, via `shared::health` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment`, via `shared::rollout` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag`, via `shared::rollout` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `(error_bps: u32)` | `set_rollback_trigger`, via `shared::rollout` |
| `rollback` | `(symbol_short!("rollback"),)` | `(ledger_sequence: u32)` | `trigger_rollback`, and `health_check` when the automatic rollback fires |
| `contract_paused` | `(Symbol::new(&env, "contract_paused"),)` — a `Symbol::new`, not `symbol_short!` | `ContractPausedEvent { admin: Address }` | `trigger_rollback`, via `shared::rollout` |

**Errors.**

`errors::MessagingError`, `#[contracterror]`, unit variants with explicit `= N` discriminants. `errors.rs` also carries a `Display` impl and a `get_suggestion` mapping to `symbol_short!` hints.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` found `DataKey::Initialized` already set in instance storage |
| `ConversationNotFound` | 2 | No `DataKey::Conversation` entry for `conv_id` — from `get_conversation`, `send_message`, `get_messages`, `mark_read`, `set_typing` |
| `MessageNotFound` | 3 | No `DataKey::Message(conv_id, seq)` entry — from `delete_message` |
| `Unauthorized` | 4 | Address is neither `participant_a` nor `participant_b` — from `send_message`, `get_messages`, `mark_read`, `set_typing`. In `get_messages` this is the only gate and the address is unauthenticated |
| `MessageTooLong` | 5 | `body.len() > MAX_MESSAGE_LEN` (4,096) in `send_message` |
| `ConvIdTooLong` | 6 | `conv_id.len() > MAX_CONV_ID_LEN` (64) in `create_conversation` |
| `MsgIdTooLong` | 7 | **Never constructed.** `MAX_MSG_ID_LEN` is referenced by no call site, so this variant is only mapped in `Display` and `get_suggestion` (`ID_LONG`) |
| `RateLimitExceeded` | 8 | `send_message` inside the window: `current_ledger < last_send.saturating_add(RATE_LIMIT_LEDGERS)`. `last_send` defaults to `0` on a first send, so a message at ledger 11 or earlier is rejected even for a never-seen sender |
| `AlreadyDeleted` | 9 | `delete_message` on a message whose `deleted` flag is already `true` |
| `CannotDeleteOthers` | 10 | `delete_message` where `msg.sender != caller` |
| `ConversationAlreadyExists` | 11 | `create_conversation` found `DataKey::Conversation(conv_id)` already present |

**Storage.** See [STORAGE.md](./STORAGE.md#messaging).

**Compile status.** `compiles` — `cargo check -p messaging --lib` succeeds. Two warnings, both dead code: `errors::get_suggestion` and `types::MAX_MSG_ID_LEN` are never used.


### multi_sig

> Multi-Signature Authorization Contract — closes #709, with the signature-validation rationale and the fail-closed posture spelled out at length in the module doc.

Source: `contracts/multi_sig/`

**Purpose.**

`multi_sig` is a reusable N-of-M authorization primitive. It owns a signer set, a threshold, and a signature lifetime, and runs a small state machine over them: `configure` installs the set, `open_proposal` puts an opaque action payload up for signature, `approve` collects one signature from one authorised signer, and `finalize` promotes the proposal to `Approved` once the threshold is reached. `2-of-3` and `3-of-5` are named in the issue as the motivating shapes but nothing is specialised to them — the state machine is generic over the configured set. The signature mechanism is `signer.require_auth()`, so the host proves the account's signature over the invocation itself; there is deliberately no "submit a signature on behalf of someone else" path, and only the fact of approval (keyed by `(proposal, signer)`) is stored — never a private key or transferable signature blob. Expiry is by ledger sequence and is terminal, and a signer set that could never meet its own threshold is rejected at configuration time rather than discovered at use time.

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — used for the health/rollout surface. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with the `testutils` feature as a dev-dependency. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. `semver_types.rs` is not a dependency: it is `include!`d from `contracts/semver_types.rs` at `lib.rs:74`.

**Public interface.**

33 externally callable `pub fn`s. The seven private helpers `has_admin`, `load_config`, `validate_config`, `is_signer_in`, `load_proposal`, `save_proposal`, and `approval_key` are free functions **outside** the `#[contractimpl]` block and are omitted. The three `get_version` / `get_version_metadata` / `is_version_compatible` rows come from the `impl_semver_queries!()` macro invocation at `lib.rs:165`.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), MultiSigError>` | `admin.require_auth()` | `ms_init` | Single-shot: guards on `has_admin`, so a second call yields `AlreadyInitialized`. The `ms_init` payload is a bare `admin` value, not a tuple |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | Macro-generated |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Macro-generated; `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated |
| `configure` | `configure(env: Env, admin: Address, signers: Vec<Address>, threshold: u32, expiry_ledgers: u32) -> Result<(), MultiSigError>` | `admin.require_auth()` **and** the parameter must equal the stored `DataKey::Admin` | `ms_cfg` | The one entry point that correctly compares the authorized parameter against the stored admin. Reads the stored admin **before** `require_auth`, so a pre-`initialize` call reports `NotInitialized` rather than an auth error. Payload is `(signers.len(), threshold, expiry_ledgers)` — the addresses themselves are not published. Duplicate detection is an O(n²) scan bounded by `MAX_SIGNERS` |
| `open_proposal` | `open_proposal(env: Env, creator: Address, proposal_id: Bytes, action: Bytes) -> Result<Proposal, MultiSigError>` | `creator.require_auth()` | `ms_prop` | Any address may open a proposal (no creator allowlist). Requires a configured set (`NotInitialized`), a non-empty `action` (`InvalidAction`), and a fresh `proposal_id` (`ProposalExists`). `expires_ledger = created_ledger.checked_add(config.expiry_ledgers)`; the `checked_add` overflow maps to `InvalidExpiry`. Increments `ProposalCount` with a plain `+ 1` |
| `approve` | `approve(env: Env, proposal_id: Bytes, signer: Address) -> Result<Proposal, MultiSigError>` | `signer.require_auth()` — this **is** the signature check | `ms_appr` | The host verifies that account's signature over this invocation, binding the approval to this proposal and its `action`. Gates, in order: configured set (`NotInitialized`), signer membership (`NotASigner`), proposal exists (`ProposalNotFound`), status is `Pending` (`ProposalClosed`), `now < expires_ledger` (`ProposalExpired`), no prior approval (`DuplicateSigner`). Stores the ledger, bumps TTL, `checked_add`s `signature_count` (overflow ⇒ `ThresholdNotMet`). Returns the whole updated `Proposal` |
| `finalize` | `finalize(env: Env, proposal_id: Bytes, executor: Address) -> Result<ProposalStatus, MultiSigError>` | `executor.require_auth()` | `ms_apprd` on success, `ms_expd` on the expiry path | `executor` need not be a signer or the admin. Fails closed: non-`Pending` ⇒ `ProposalClosed`; elapsed window ⇒ sets `Expired`, saves, emits `ms_expd`, **then** returns `ProposalExpired`; under threshold ⇒ `ThresholdNotMet` and the proposal is left untouched. Expiry is terminal — `ms_expd`'s payload is the bare `proposal_id` |
| `get_config` | `get_config(env: Env) -> Result<MultiSigConfig, MultiSigError>` | none (view) | — | `NotInitialized` before `configure` |
| `is_signer` | `is_signer(env: Env, address: Address) -> Result<bool, MultiSigError>` | none (view) | — | Linear scan of the signer set; `NotInitialized` before `configure` |
| `has_approved` | `has_approved(env: Env, proposal_id: Bytes, signer: Address) -> bool` | none (view) | — | `persistent().has(..)`; `false` for an unknown proposal |
| `get_approval` | `get_approval(env: Env, proposal_id: Bytes, signer: Address) -> Option<u32>` | none (view) | — | The ledger the signature landed on; the entire content of a "signature" |
| `get_signature_count` | `get_signature_count(env: Env, proposal_id: Bytes) -> Result<u32, MultiSigError>` | none (view) | — | Count only; does not verify the collected signers still match the current set after a `configure` |
| `get_proposal` | `get_proposal(env: Env, proposal_id: Bytes) -> Result<Proposal, MultiSigError>` | none (view) | — | Full record, including the opaque `action` payload, readable by anyone |
| `is_approved` | `is_approved(env: Env, proposal_id: Bytes) -> bool` | none (view) | — | `true` only for `status == Approved`; `false` for a missing proposal |
| `get_proposal_count` | `get_proposal_count(env: Env) -> u32` | none (view) | — | Defaults to `0`; monotonically increasing, never decremented |
| `health_check` | `health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt` (conditional), `rollback` (conditional) | Auto-rollback on anomaly. Note the health metric counters are only advanced by the explicit `report_ok` / `report_error` calls — the contract's own entry points do not report their outcomes |
| `get_health_metrics` | `get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | Includes the live `paused` flag |
| `get_sla_targets` | `get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | `env` explicitly discarded |
| `set_alert_config` | `set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` (on the **parameter**) | `alrt_cfg` | The parameter is authorized but **not compared to the stored `DataKey::Admin`** — any address can overwrite the alert config. Panics inside `shared` on bps > 10000, degraded > unhealthy, or zero stall/cooldown |
| `get_alert_config` | `get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to `default_alert_config()` |
| `detect_anomaly` | `detect_anomaly(env: Env) -> bool` | none (view) | — | Read-only |
| `report_ok` | `report_ok(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | — | Unchecked-parameter gap; stamps `last_ok_ledger` |
| `report_error` | `report_error(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | — | Unchecked-parameter gap; feeds `should_rollback` |
| `set_feature_flag` | `set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` (on the parameter) | `feat_flg` | Unchecked-parameter gap; appends to `FlagIndex` if absent |
| `is_feature_enabled` | `is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | Forced `false` while phase is `RolledBack` |
| `set_canary_deployment` | `set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` (on the parameter) | `canary` | Unchecked-parameter gap; panics above 10,000 bps |
| `route_to_canary` | `route_to_canary(env: Env, caller: Address) -> bool` | **no `require_auth` — `none`** | — | `caller` is unauthenticated; read-only |
| `get_rollout_state` | `get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | Aggregates phase, bps, canary/stable, rollback bps, flag count |
| `set_rollback_trigger` | `set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` (on the parameter) | `rb_trig` | Unchecked-parameter gap; panics on `0` or `> 10_000` |
| `should_rollback` | `should_rollback(env: Env) -> bool` | none (view) | — | `true` once phase is `RolledBack`, else error-rate vs trigger |
| `trigger_rollback` | `trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` (on the parameter) | `rollback`, `contract_paused` | Zeroes `CanaryBps`, sets `RolledBack`, disables all indexed flags, sets `Paused = true` |

**Events.**

Six contract-specific shapes plus seven reached through the delegated `shared` health/rollout entry points.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `ms_init` | `(symbol_short!("ms_init"),)` | `admin: Address` — a single value, not a tuple | `initialize` |
| `ms_cfg` | `(symbol_short!("ms_cfg"),)` | `(signers_len: u32, threshold: u32, expiry_ledgers: u32)` — signer addresses are deliberately not published | `configure` |
| `ms_prop` | `(symbol_short!("ms_prop"),)` | `(proposal_id: Bytes, creator: Address, expires_ledger: u32)` | `open_proposal` |
| `ms_appr` | `(symbol_short!("ms_appr"),)` | `(proposal_id: Bytes, signer: Address, signature_count: u32, threshold: u32)` | `approve` |
| `ms_expd` | `(symbol_short!("ms_expd"),)` | `proposal_id: Bytes` — a single value, not a tuple | `finalize`, on the elapsed-window path (published *before* the `ProposalExpired` error is returned) |
| `ms_apprd` | `(symbol_short!("ms_apprd"),)` | `(proposal_id: Bytes, signature_count: u32)` | `finalize`, on the success path |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health_check`, via `shared::health` — suppressed inside the cooldown window |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `(unhealthy_error_bps: u32)` | `set_alert_config`, via `shared::health` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `set_canary_deployment`, via `shared::rollout` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `set_feature_flag`, via `shared::rollout` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `(error_bps: u32)` | `set_rollback_trigger`, via `shared::rollout` |
| `rollback` | `(symbol_short!("rollback"),)` | `(ledger_sequence: u32)` | `trigger_rollback`, and `health_check` when the automatic rollback fires |
| `contract_paused` | `(Symbol::new(&env, "contract_paused"),)` — a `Symbol::new`, not `symbol_short!` | `ContractPausedEvent { admin: Address }` | `trigger_rollback`, via `shared::rollout` |

**Errors.**

`errors::MultiSigError`, `#[contracterror]`, unit variants with explicit `= N` discriminants. `src/test.rs::error_codes_are_stable` pins every discriminant so a renumbering fails the build. `errors.rs` also carries a `Display` impl and a `get_suggestion` mapping.

| Variant | Code | Condition |
|---|---|---|
| `NotInitialized` | 1 | No `DataKey::Config` — from `configure` (before `initialize`) and from `load_config` inside `open_proposal`, `approve`, `finalize`, `get_config`, `is_signer` |
| `AlreadyInitialized` | 2 | `initialize` found `DataKey::Admin` already present |
| `Unauthorized` | 3 | `configure` where the authorized `admin` parameter does not match the stored `DataKey::Admin` |
| `InvalidSigners` | 4 | `configure` with an empty set, `len() > MAX_SIGNERS` (10), or a duplicated address found by the O(n²) scan |
| `InvalidThreshold` | 5 | `configure` with `threshold < MIN_THRESHOLD` (2) or `threshold > signers.len()` |
| `InvalidExpiry` | 6 | `configure` with `expiry_ledgers == 0` or `> MAX_EXPIRY_LEDGERS` (120,960); also `open_proposal` when `created_ledger.checked_add(expiry_ledgers)` overflows |
| `ProposalNotFound` | 7 | No `DataKey::Proposal` for the id — from `approve`, `finalize`, `get_signature_count`, `get_proposal` |
| `ProposalExists` | 8 | `open_proposal` where the `proposal_id` is already taken |
| `NotASigner` | 9 | `approve` where `signer` is not in the configured set |
| `DuplicateSigner` | 10 | `approve` where `DataKey::Approval(id, signer)` already exists |
| `ThresholdNotMet` | 11 | `finalize` with `signature_count < threshold`; also `approve` when `checked_add` on `signature_count` overflows |
| `ProposalExpired` | 12 | `approve` where `now >= expires_ledger`; `finalize` where `now >= expires_ledger` (status set to `Expired` and saved first) |
| `ProposalClosed` | 13 | `approve` or `finalize` on a proposal whose `status != Pending` — including one already marked `Expired`, which is what makes expiry terminal |
| `InvalidAction` | 14 | `open_proposal` with `action.len() == 0` |

**Storage.** See [STORAGE.md](./STORAGE.md#multi-sig).

**Compile status.** `compiles` — `cargo check -p multi_sig --lib` succeeds. The `#[cfg(test)] mod test` in `src/test.rs` (29 tests, including the error-code stability pin) compiles against the `rlib` target.


### nft

> This crate has no `//!` module doc comment. Nearest header comment (contracts/nft/src/lib.rs:1-9): "Implements NFT Ownership Certificates — closes #614. Acceptance Criteria: Support NFT minting for completed commissions; Implement royalty tracking; Add secondary sale support; Track NFT ownership history; Implement NFT burn/transfer restrictions."

Source: `contracts/nft/`

**Purpose.**

Mints and tracks on-chain NFT *ownership certificates* that stand in for completed creative commissions, so a creator can prove ownership of a work and a buyer can acquire that ownership with an automatic royalty to the original artist. Each certificate is a `NftCertificate` record carrying title, an off-chain `metadata_uri`, creator/owner, royalty basis points, an optional linked `commission_id`, and per-certificate `transferable` / `burnable` flags that act as the transfer and burn restrictions. Ownership changes append an immutable `TransferRecord` provenance chain (from, to, price, ledger), and a secondary sale with `price > 0` and `royalty_bps > 0` automatically moves `price * royalty_bps / 10_000` of the sale token to the creator before the `owner` field is updated. A single stored `Admin` address — the platform operator — holds one extra power: `freeze` can force a certificate non-transferable, for IP disputes or fraudulent mints. Queries are read-only; there is no listing, offer, or royalty-claim mechanism, and this crate exposes none of the shared health/pause/rollout surface.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — **declared but never used**: no `shared::` path appears anywhere in `src/lib.rs`. Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to `21.7.7` in `Cargo.lock`); the dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, royalty_token: Address)` | `admin.require_auth()` | `init` | One-shot setup writing `Admin`, `RoyaltyToken`, and `NftCount = 0`. **Not idempotent-guarded** — a second call silently overwrites the admin and the royalty token and resets the id counter to `0`, which would collide with existing `DataKey::Nft(id)` records. Emits `(init,)` with the admin as payload. |
| `mint` | `fn mint(env: Env, creator: Address, owner: Address, title: String, metadata_uri: String, royalty_bps: u32, commission_id: u64, transferable: bool, burnable: bool) -> u64` | `creator.require_auth()` — the artist signs, not the admin | `minted` | Doc comment says the platform "typically" mints on the creator's behalf, but the auth requirement is the `creator` address itself. Asserts `royalty_bps <= 3000`. Ids come from the internal `next_id`; the new record gets `status: Active` and `minted_at: env.ledger().sequence()`. `commission_id == 0` means standalone. Emits `(minted, title)` with payload `(id, creator, owner)`. Returns the new id. |
| `transfer` | `fn transfer(env: Env, from: Address, to: Address, nft_id: u64, price: i128)` | `from.require_auth()` | `transfer` | Requires `status == Active`, `owner == from`, and `transferable == true`; asserts `price >= 0`. On a secondary sale (`price > 0 && royalty_bps > 0`) it computes `price * royalty_bps / 10_000` and moves it with `token::Client::transfer(&from, &nft.creator, &royalty_amount)` — the royalty is debited from the **seller** (`from`), not from the buyer (`to`); the inline comment "Transfer royalty from the buyer (caller context) to creator" does not match the code, and the buyer never signs. Then appends the `TransferRecord`, increments `TransferCount`, and updates `owner`. `price` is recorded but never verified against an actual payment. |
| `burn` | `fn burn(env: Env, owner: Address, nft_id: u64)` | `owner.require_auth()` | `burned` | Requires `status == Active`, `nft.owner == owner` (the caller parameter), and `burnable == true`. Sets `status = Burned` only; the record and its whole provenance chain stay in storage, and a burned certificate can never be transferred or burned again. |
| `freeze` | `fn freeze(env: Env, admin: Address, nft_id: u64)` | `admin.require_auth()`, plus role check `admin == stored DataKey::Admin` | `frozen` | The only admin-gated mutation. Sets `transferable = false` on an existing record — irreversible in this contract: there is no unfreeze, and `update_royalty` cannot restore the flag because it writes only `royalty_bps`. It does not check the certificate status, so a `Burned` certificate can also be frozen. |
| `update_royalty` | `fn update_royalty(env: Env, creator: Address, nft_id: u64, new_royalty_bps: u32)` | `creator.require_auth()`, plus role check `creator == nft.creator` | `royalty` | Asserts `new_royalty_bps <= 3000`, `nft.creator == creator`, and `status == Active`. Writes only `royalty_bps`; it does not touch already-booked `TransferRecord`s. |
| `get_nft` | `fn get_nft(env: Env, nft_id: u64) -> NftCertificate` | none (view) | — | Calls the internal `load_nft`, which uses `expect("NFT not found")`; an unknown id aborts the host call rather than returning `Option`. |
| `get_transfer_count` | `fn get_transfer_count(env: Env, nft_id: u64) -> u64` | none (view) | — | Reads `DataKey::TransferCount(nft_id)`, defaulting to `0`. |
| `get_transfer_history` | `fn get_transfer_history(env: Env, nft_id: u64, offset: u64, limit: u64) -> Vec<TransferRecord>` | none (view) | — | Paginated slice bounded by `(offset + limit).min(TransferCount)`; silently skips indices whose record is missing. A `limit` of `0` returns an empty vector. |

Internal (not part of the external interface): `next_id` and `load_nft` are private `fn`s inside the `#[contractimpl]` block and are not callable cross-contract.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `admin` | `initialize` |
| `minted` | `[contract, "minted", title]` | `(id, creator, owner)` | `mint` |
| `transfer` | `[contract, "transfer", nft_id]` | `(from, to, price)` | `transfer` |
| `burned` | `[contract, "burned"]` | `(nft_id, owner)` | `burn` |
| `frozen` | `[contract, "frozen"]` | `(nft_id, admin)` | `freeze` |
| `royalty` | `[contract, "royalty"]` | `(nft_id, creator, new_royalty_bps)` | `update_royalty` |

Each shape is emitted from exactly one site; there are no repeats. This crate publishes no events other than these six — in particular the SEP-41 `transfer` performed during royalty settlement originates from the token contract, not from `NftOwnership`.

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| no `#[contracterror]` enum — all failures are host panics | n/a | Every rejection is a string-literal `assert!` or an `Option::expect`, so failures surface as `HostError` with a `contract panic` (code 0x...-1 style trap), not as a typed contract error. In `initialize` there is no failure path at all. In `mint`: `"royalty_bps must not exceed 3000 (30%)"`. In `transfer`: `"price must be non-negative"`, `expect("contract not initialized")` for `RoyaltyToken`, `"NFT is not active"`, `"caller is not the NFT owner"`, `"this NFT cannot be transferred"`, plus any panic raised by the SEP-41 `token::transfer` (e.g. insufficient balance/allowance). In `burn`: `expect("NFT not found")`, `"NFT is not active"`, `"caller is not the NFT owner"`, `"this NFT cannot be burned"`. In `freeze`: `expect("contract not initialized")`, `expect("NFT not found")`, `"only admin can freeze"`. In `update_royalty`: `"royalty_bps must not exceed 3000 (30%)"`, `expect("NFT not found")`, `"only the creator can update royalties"`, `"NFT is not active"`. In the internal `load_nft`: `expect("NFT not found")`. |

**Storage.** See [STORAGE.md](./STORAGE.md#nft).

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p nft` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The crate builds a `cdylib` + `rlib`, so `NftOwnership` and all nine entry points above are reachable. Two behavioural caveats that compile cleanly but are worth flagging: the royalty on a secondary sale is debited from the seller `from` rather than the buyer `to` (the code and its comment disagree), and `initialize` is not guarded against re-invocation even though it is the only writer of the id counter.


### platform_config

> "Platform Configuration Contract — Protocol-wide parameters, fee governance, and admin authority delegation. Architecture Decision: [ADR-0005](../../docs/ADRs/0005-platform-fee-and-revenue-distribution.md)" (`//!` doc comment, contracts/platform_config/src/lib.rs:1-4)

Source: `contracts/platform_config/`

**Purpose.**

This is the workspace's configuration / kill-switch registry: the single on-chain source of truth for the protocol fee (`FeeBps`), the fee-receiving `PlatformWallet`, the `UsdcToken`, the two-step `Admin` / `PendingAdmin` handover, and the fee-token metadata bounds `MinFeeBps` / `MaxFeeBps`. On top of that it governs a namespaced address registry (`Production` / `Test` × arbitrary `Symbol` name) with a ledger-stamped resolution cache, and the advanced fee policy of #690: volume `FeeTiers`, a `Promotion` window, a `ReferralConfig` share, and per-payer cumulative `Volume` used to price operations. Consumers call it two ways. (a) *Registry resolution* — `resolve_for_environment` / `resolve_address` return the address registered for a name in the active (or a named) environment, which is how a contract discovers its dependencies; the `tests/framework` scenario does exactly this with the names `usdc` (tests/framework/tests/full_scenario.rs:32-39) and `escrow` (contracts/platform_config/src/tests.rs:130). (b) *Classic config reads* — `contracts/escrow` invokes the four short selectors `get_fee_b` (create_escrow and fee math), `get_usdc` (create_escrow, release_payment, partial-resolve token lookup), `get_adm` (release_payment and set_dispute_ttl_ledgers auth) and `get_pw` (release_payment payout destination), and `contracts/dispute_arbiter` invokes `get_usdc` and forwards the config address to escrow's `refund_cl` / `rel_pay`; `contracts/shared/src/config.rs` wraps those same four selectors as `try_get_fee_bps` / `try_get_usdc` / `try_get_admin` / `try_get_platform_wallet`. **This contract implements none of those four entry points** — it exposes `get_config`, `get_token_metadata`, `get_registered_address`, `registry_entry`, `get_environment`, `resolve_address`, `resolve_for_environment`, `resolution_cache`, plus the #690 and health/rollout surface — so the classic consumers cannot read it directly; `tests/framework/src/config.rs` documents the workaround by deploying a `ConfigStub` stand-in for the escrow lifecycle. It also re-exports the shared health-monitoring (#678) and gradual-rollout (#684) surface, which is where its kill switches live (`set_feature_flag`, `set_canary_deployment`, `set_rollback_trigger`, `trigger_rollback`).

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — used, via `shared::health` (metrics/alerting) and `shared::rollout` (canary, feature flags, rollback). Registry deps: `soroban-sdk = { version = "21.0.0" }` (pinned directly rather than through `soroban-sdk.workspace`; resolved to `21.7.7` in `Cargo.lock`); dev-dependency `soroban-sdk = { workspace = true, features = ["testutils"] }`. Feature `testutils = ["soroban-sdk/testutils"]`. Also splices in `contracts/semver_types.rs` via `include!("../../semver_types.rs")` (lib.rs:21), which supplies the `impl_semver_queries!` macro, `ContractVersion`, and `VersionMetadata`. It is a workspace member, so nothing outside the crate is a path dep of it; it is a path dep of `tests/framework` only.

**Public interface.**

Signature order follows the source. Items marked **UNREACHABLE** sit textually inside the unterminated `resolution_cache` function body (lib.rs:256-261) and can never be parsed as members of the `#[contractimpl]` block — see Compile status.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, fee_bps: u32, platform_wallet: Address, usdc_token: Address) -> Result<(), ConfigError>` | **none** — no `require_auth()` anywhere in the body | `init` | Returns `AlreadyInitialized` if `is_initialized`, and `InvalidFeeBps` if `fee_bps > 1000`. Sets `Admin`, `FeeBps`, `PlatformWallet`, `UsdcToken`. Because there is no auth, the *first* caller of an uninitialised deployment picks the admin — front-runnable by design gap, not by argument. |
| `get_version` | `fn get_version(_env: Env) -> ContractVersion` (from `impl_semver_queries!()`) | none (view) | — | `parse_pkg_semver(env!("CARGO_PKG_VERSION"))` → `0.1.0`. Exported as a contract entry point? unknown — the macro is expanded inside `#[contractimpl]`, and this crate's own comment at lib.rs:262-266 states "SDK-21 does not export macro-generated (`impl_semver_queries!`) functions". |
| `get_version_metadata` | `fn get_version_metadata(env: Env) -> VersionMetadata` (macro) | none (view) | — | Returns crate name, semver, `min_compatible`, and `storage_schema: 1`. Same export caveat as `get_version`. |
| `is_version_compatible` | `fn is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` (macro) | none (view) | — | Compares the client-required triple against `0.1.0`; for `major == 0` requires equal minor and `patch >=` required. Same export caveat. |
| `get_config` | `fn get_config(env: Env) -> PlatformConfig` | none (view) | — | **Not a pure view**: on an instance-cache miss it assembles the four fields and *writes* `cfgcache`. Returns `{ admin, fee_bps, platform_wallet, usdc_token }`. This is the closest thing this contract has to the legacy `get_adm` / `get_fee_b` / `get_pw` / `get_usdc` selectors, but it is a different signature, so escrow's `invoke_contract` calls cannot be pointed at it. |
| `set_fee_bps` | `fn set_fee_bps(env: Env, fee_bps: u32) -> Result<(), ConfigError>` | `admin.require_auth()` — the stored `Admin`, read inside the function | `feeupdtd` | Rejects `> 1000` with `InvalidFeeBps`. Emits `(old_fee, fee_bps)` so a fee change is auditable. Invalidates `cfgcache`. |
| `set_platform_wallet` | `fn set_platform_wallet(env: Env, platform_wallet: Address) -> Result<(), ConfigError>` | `admin.require_auth()` | — | No event is published, so a wallet change leaves no on-chain trace beyond the storage write; invalidates `cfgcache`. |
| `transfer_admin` | `fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ConfigError>` | `admin.require_auth()` (outgoing admin) | `admprosd` | Step 1 of the two-step handover: writes `PendingAdmin` only; the admin is unchanged until `accept_admin`. |
| `accept_admin` | `fn accept_admin(env: Env) -> Result<(), ConfigError>` | `pending.require_auth()` — the *incoming* admin signs | `admtxfrd` | Step 2: `get_pending_admin().ok_or(NoPendingAdmin)?`, promotes the pending address to `Admin`, invalidates `cfgcache`. Note there is no expiry or cancellation path for a pending proposal, and the old admin can overwrite it by calling `transfer_admin` again. |
| `set_token_metadata` | `fn set_token_metadata(env: Env, name: String, symbol: String, decimal: u32, min_fee_bps: u32, max_fee_bps: u32) -> Result<(), ConfigError>` | `admin.require_auth()` | `tkmeta` | Writes all five token-metadata keys at once; rejects `max_fee_bps > 1000` with `InvalidFeeBps`. `min_fee_bps` is *not* validated against `max_fee_bps` here, so an inverted range is storable; the clamp in `fees::resolve_effective_fee_bps` uses `clamp`, which panics if `min > max`. Payload is the empty tuple. |
| `get_token_metadata` | `fn get_token_metadata(env: Env) -> FeeTokenMetadata` | none (view) | — | Reassembles `{ name, symbol, decimal, min_fee_bps, max_fee_bps }` from the five separate instance keys; the four non-string fields use `unwrap`/`unwrap_or`, so calling it before `set_token_metadata` aborts the host call. |
| `register_address` | `fn register_address(env: Env, e: AddressEnvironment, name: Symbol, address: Address) -> Result<(), ConfigError>` | `admin.require_auth()` | `addrreg` | The registry's write path: upserts `RegistryEntry(e, name)` and removes any `ResolutionCache(e, name)` so the new address takes effect immediately. Names are unvalidated `Symbol`s. |
| `unregister_address` | `fn unregister_address(env: Env, e: AddressEnvironment, name: Symbol) -> Result<(), ConfigError>` | `admin.require_auth()` | — | Errors `AddressNotRegistered` on a miss rather than silently no-opping, then removes both the entry and its cache. Emits nothing. |
| `get_registered_address` | `fn get_registered_address(env: Env, e: AddressEnvironment, name: Symbol) -> Result<Address, ConfigError>` | none (view) | — | Direct registry read, bypassing the cache. Errors `AddressNotRegistered`. |
| `registry_entry` | `fn registry_entry(env: Env, e: AddressEnvironment, name: Symbol) -> Result<RegistryEntry, ConfigError>` | none (view) | — | Returns `{ env, name, address }` for tooling/audits. |
| `set_environment` | `fn set_environment(env: Env, e: AddressEnvironment) -> Result<(), ConfigError>` | `admin.require_auth()` | — | Switches the namespace `resolve_for_environment` reads. Emits nothing. |
| `get_environment` | `fn get_environment(env: Env) -> AddressEnvironment` | none (view) | — | Reads `ActiveEnvironment`, defaulting to `Production`. |
| `resolve_address` | `fn resolve_address(env: Env, e: AddressEnvironment, name: Symbol) -> Result<Address, ConfigError>` | none (view) | — | **Not a pure view**: on a stale or missing cache entry it *writes* a fresh `ResolutionCache(e, name)` stamp. Returns the cached address while `now >= resolved_ledger && now - resolved_ledger <= RESOLUTION_CACHE_TTL_LEDGERS`; otherwise re-reads the registry (`AddressNotRegistered` if absent). |
| `resolve_for_environment` | `fn resolve_for_environment(env: Env, name: Symbol) -> Result<Address, ConfigError>` | none (view) | — | Reads `ActiveEnvironment` and delegates to `resolve_address`, so it also populates the cache. This is the entry point `tests/framework/src/assertions.rs:38` and the full-scenario test use. |
| `resolution_cache` | `fn resolution_cache(env: Env, e: AddressEnvironment, name: Symbol) -> Result<ResolutionCacheEntry, ConfigError>` | none (view) | — | **UNREACHABLE — the function is missing its closing `}` at lib.rs:261**, so its body is never closed and the remainder of the impl block is swallowed into it. Intended behaviour: return the stored `{ address, resolved_ledger }` or `AddressNotRegistered`. |
| `upsert_fee_tier` | `fn upsert_fee_tier(env: Env, admin: Address, tier: FeeTier) -> Result<bool, ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` → else `Unauthorized` | `tier` | **UNREACHABLE.** Intended: reject `tier.min_volume < 0 \|\| tier.fee_bps > 1000` with `InvalidTier`; insert or replace the tier keeping `FeeTiers` sorted ascending by `min_volume`; return `true` when a tier was replaced. |
| `remove_fee_tier` | `fn remove_fee_tier(env: Env, admin: Address, min_volume: i128) -> Result<bool, ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` | `tierrm` | **UNREACHABLE.** Intended: drop the tier with that threshold, return whether one was removed. |
| `get_fee_tiers` | `fn get_fee_tiers(env: Env) -> Vec<FeeTier>` | none (view) | — | **UNREACHABLE.** Intended: the ladder sorted ascending by `min_volume`, empty vector when unset. |
| `set_promotion` | `fn set_promotion(env: Env, admin: Address, promotion: Promotion) -> Result<(), ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` | `promo` | **UNREACHABLE.** Intended: reject `end_ledger < start_ledger` or `fee_bps > 1000` with `InvalidPromotion`, then store the window. |
| `clear_promotion` | `fn clear_promotion(env: Env, admin: Address) -> Result<(), ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` | `promo` + `clr` | **UNREACHABLE.** Intended: remove the `Promotion` entry. |
| `set_referral_config` | `fn set_referral_config(env: Env, admin: Address, config: ReferralConfig) -> Result<(), ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` | `refcfg` | **UNREACHABLE.** Intended: reject `config.bps > 10_000` with `InvalidReferralBps`; store the referrer share of the platform fee. |
| `get_referral_config` | `fn get_referral_config(env: Env) -> Option<ReferralConfig>` | none (view) | — | **UNREACHABLE.** Intended: `None` when unset. |
| `record_volume` | `fn record_volume(env: Env, admin: Address, payer: Address, amount: i128) -> Result<i128, ConfigError>` | `admin.require_auth()`, plus role check `admin == stored Admin` | `vol` | **UNREACHABLE.** Intended: reject `amount < 0` (with `InvalidFeeBps` — the wrong error code for a negative amount), saturating-add into `Volume(payer)`, renew that key's TTL, and return the new total. The doc comment says "the escrow contract is expected to call this on release", but escrow does not: no caller of `record_volume` exists outside this crate's own tests. |
| `get_volume` | `fn get_volume(env: Env, payer: Address) -> i128` | none (view) | — | **UNREACHABLE.** Intended: cumulative volume, `0` when never recorded. |
| `resolve_effective_fee_bps` | `fn resolve_effective_fee_bps(env: Env, volume: i128) -> u32` | none (view) | — | **UNREACHABLE.** Intended: pure resolution of base fee vs. active promotion vs. matching volume tier, clamped to `[MinFeeBps, MaxFeeBps]`. In `fees::resolve_effective_fee_bps` an *out-of-window* promotion falls through to the tier lookup, so a stored-but-expired `Promotion` behaves as if absent. |
| `compute_fees` | `fn compute_fees(env: Env, amount: i128, volume: i128, referrer: Option<Address>) -> Result<FeeBreakdown, ConfigError>` | none (view) | — | **UNREACHABLE.** Intended: the cross-contract pricing call — returns `{ effective_fee_bps, amount, fee, payout, referral_fee, platform_fee }`. The referral split is enabled purely by passing `Some(_)`; the referrer's address is never used, and the result still has to be settled by the caller. All `fees::compute` arithmetic failures (negative `amount`, checked-mul/sub overflow) are collapsed into `InvalidFeeBps`. |
| `is_promotion_active` | `fn is_promotion_active(env: Env) -> bool` | none (view) | — | **UNREACHABLE.** Intended: `true` while `start_ledger <= now <= end_ledger`, `false` when no promotion is stored. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt`, `rollback` | **UNREACHABLE.** Intended: classify the contract against its SLA thresholds and, if `report.anomaly`, call `shared::rollout::maybe_auto_rollback`. Side-effecting despite the "check" name. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | **UNREACHABLE.** Intended: `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | **UNREACHABLE.** Intended: the published SLA constants so monitors need not hard-code them. `env` is bound and discarded. |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `alrt_cfg` | **UNREACHABLE.** Intended: persist alerting thresholds; `shared::health::set_alert_config` panics `"invalid alert config"` if the bps values exceed `10_000`, are inverted, or `stall_ledgers` / `alert_cooldown_ledgers` is `0`. Note the auth is on the *passed* `admin`, which is not compared against the stored `Admin`. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | **UNREACHABLE.** Intended: stored config, or `default_alert_config()` when unset. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | **UNREACHABLE.** Intended: `true` when degraded, unhealthy, or stalled. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` | — | **UNREACHABLE.** Intended: increment `ok_count` and stamp `last_ok_ledger`. Off-chain monitors/operators drive these counters; nothing in the crate calls them automatically. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` | — | **UNREACHABLE.** Intended: increment `error_count` and stamp `last_error_ledger`. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` | `feat_flg` | **UNREACHABLE.** Intended: write `Flag(flag)` and append the name to `FlagIndex` on first use. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | **UNREACHABLE.** Intended: the flag value, forced to `false` while the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `canary` | **UNREACHABLE.** Intended: register both ids and the traffic share; panics `"canary_bps exceeds 10000"` above the cap and derives `Phase` from the bps. |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | **UNREACHABLE.** Intended: sticky per-caller split — `SHA-256(caller XDR)` mod `10_000` `< canary_bps`; always `false` after a rollback. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | **UNREACHABLE.** Intended: `{ phase, canary_bps, canary, stable, rollback_error_bps, flag_count }`. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `rb_trig` | **UNREACHABLE.** Intended: arm automatic rollback; panics `"invalid rollback trigger"` on `0` or `> 10_000`. |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | **UNREACHABLE.** Intended: `true` when the health error rate meets the trigger and at least one sample exists, or the phase is already `RolledBack`. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` | `rollback`, `contract_paused` | **UNREACHABLE.** Intended: zero `canary_bps`, set `Phase = RolledBack`, force every indexed flag to `false`, and set `PauseDataKey::Paused = true`. Because this contract exposes no unpause path, the pause flag it sets can never be cleared through this contract. |

Internal (not part of the external interface): `invalidate_config_cache` is a private `fn` in the `#[contractimpl]` block. `storage.rs`, `fees.rs`, `errors.rs`, and `types.rs` are all `pub mod`, so their helpers (`get_admin`, `set_fee_bps_val`, `upsert_fee_tier`, `resolve_effective_fee_bps`, `compute`, `get_suggestion`, …) are reachable as Rust library items from a host build but are **not** contract entry points.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, fee_bps)` | `initialize` |
| `feeupdtd` | `[contract, "feeupdtd"]` | `(old_fee, fee_bps)` | `set_fee_bps` |
| `admprosd` | `[contract, "admprosd"]` | `new_admin` | `transfer_admin` |
| `admtxfrd` | `[contract, "admtxfrd"]` | `pending` | `accept_admin` |
| `tkmeta` | `[contract, "tkmeta"]` | `()` — empty tuple, no data | `set_token_metadata` |
| `addrreg` | `[contract, "addrreg", name]` | `(e, address)` | `register_address` |
| `tier` | `[contract, "tier"]` | `(tier.min_volume, tier.fee_bps)` | `upsert_fee_tier` — UNREACHABLE |
| `tierrm` | `[contract, "tierrm"]` | `min_volume` | `remove_fee_tier` — UNREACHABLE |
| `promo` | `[contract, "promo"]` | `(promotion.start_ledger, promotion.end_ledger, promotion.fee_bps)` | `set_promotion` — UNREACHABLE |
| `promo` + `clr` | `[contract, "promo", "clr"]` | `()` — empty tuple | `clear_promotion` — UNREACHABLE |
| `refcfg` | `[contract, "refcfg"]` | `config.bps` | `set_referral_config` — UNREACHABLE |
| `vol` | `[contract, "vol"]` | `(payer, amount, total)` | `record_volume` — UNREACHABLE |
| `alrt_cfg` | `[contract, "alrt_cfg"]` | `config.unhealthy_error_bps` | `shared::health::set_alert_config` via `set_alert_config` — UNREACHABLE |
| `hlth_alrt` | `[contract, "hlth_alrt"]` | `(report.status, report.error_bps, report.stalled)` | `shared::health::health_check` via `health_check`, only when an anomaly is detected, alerting is enabled, and the cooldown has elapsed — UNREACHABLE |
| `canary` | `[contract, "canary"]` | `(canary, stable, canary_bps)` | `shared::rollout::set_canary_deployment` via `set_canary_deployment` — UNREACHABLE |
| `feat_flg` | `[contract, "feat_flg"]` | `(flag, enabled)` | `shared::rollout::set_feature_flag` via `set_feature_flag` — UNREACHABLE |
| `rb_trig` | `[contract, "rb_trig"]` | `error_bps` | `shared::rollout::set_rollback_trigger` via `set_rollback_trigger` — UNREACHABLE |
| `rollback` | `[contract, "rollback"]` | `env.ledger().sequence()` | `shared::rollout::apply_rollback`, reached from `trigger_rollback` and from `health_check` → `maybe_auto_rollback` — UNREACHABLE |
| `contract_paused` | `[contract, "contract_paused"]` | `shared::pause::ContractPausedEvent { admin }` | `shared::rollout::trigger_rollback` via `trigger_rollback` — UNREACHABLE |

`unregister_address`, `set_platform_wallet`, and `set_environment` emit nothing. No event shape is published from more than one site, so there are no repeats to collapse.

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` when `is_initialized` is already true (i.e. `Admin` exists). |
| `NotInitialized` | 2 | **Never constructed** in this crate. Every getter unwraps instead — `get_admin`, `get_fee_bps`, `get_platform_wallet`, `get_usdc`, `get_token_name`, `get_token_symbol`, `get_token_decimal` all use `.unwrap()`, so calling a contract before `initialize` aborts the host call with a panic rather than returning this typed error. |
| `Unauthorized` | 3 | Returned only by the #690 admin-gated setters (`upsert_fee_tier`, `remove_fee_tier`, `set_promotion`, `clear_promotion`, `set_referral_config`, `record_volume`) when the `admin` that just signed differs from the stored `Admin`. All of those are UNREACHABLE. Note the legacy setters (`set_fee_bps`, `set_platform_wallet`, `set_token_metadata`, `register_address`, `unregister_address`, `set_environment`) instead `require_auth()` the *stored* admin and so can never return it. |
| `InvalidFeeBps` | 4 | `initialize` or `set_fee_bps` with `fee_bps > 1000`; `set_token_metadata` with `max_fee_bps > 1000`; `record_volume` with `amount < 0` (the mismatched reuse for a negative amount); and every arithmetic failure inside `compute_fees` (`fees::compute` returns `FeeComputationError::ArithmeticOverflow`, which is mapped here with `.map_err(\|_\| ConfigError::InvalidFeeBps)`). |
| `NoPendingAdmin` | 5 | `accept_admin` when `PendingAdmin` is unset. |
| `AddressNotRegistered` | 6 | `unregister_address`, `get_registered_address`, `registry_entry`, `resolve_address`, and `resolution_cache` when `(e, name)` is absent from the registry (or the cache). **Duplicate discriminant:** `InvalidTier` also has code 6 (see below). |
| `InvalidTier` | 6 | `upsert_fee_tier` when `tier.min_volume < 0` or `tier.fee_bps > 1000`. UNREACHABLE. Shares code 6 with `AddressNotRegistered`, so the two are indistinguishable on the wire; the macro-generated decode match hits the `AddressNotRegistered` arm first and the `InvalidTier` arm is an unreachable pattern. The duplicate is a warning under plain `cargo check`, but `make lint` runs `cargo clippy --all-targets -- -D warnings`, which would promote it to an error. |
| `InvalidPromotion` | 7 | `set_promotion` when `promotion.end_ledger < promotion.start_ledger` or `promotion.fee_bps > 1000`. UNREACHABLE. |
| `InvalidReferralBps` | 8 | `set_referral_config` when `config.bps > 10_000`. UNREACHABLE. |
| `PromotionNotActive` | 9 | **Never constructed** in this crate — `is_promotion_active` returns a `bool` and the promo branches of `resolve_effective_fee_bps` / `compute_fees` have no error path. |

**Storage.** See [STORAGE.md](./STORAGE.md#platform-config).

**Compile status.** `fails to compile`  > **Compile status:** verified with `cargo check -p platform_config`, which fails with exactly one diagnostic: `error: this file contains an unclosed delimiter` — pointing at `contracts/platform_config/src/lib.rs:492:12` (`mod tests;`), reporting `27 | impl PlatformConfigContract {` as the unclosed delimiter and `260 | ) -> Result<ResolutionCacheEntry, ConfigError> {` as the one "that might not be properly closed". The `pub fn resolution_cache` at lib.rs:256-261 ends on `.ok_or(ConfigError::AddressNotRegistered)` with no closing brace, so its body is never terminated. > > **Consequences.** (1) Parsing aborts at the crate root, so **no artifact is produced at all** — neither `cdylib` nor `rlib`, hence no `PlatformConfigContractClient` and no deployable WASM. Every entry point in the table above is unavailable in practice, including the ones that parse correctly. (2) Within the source text, everything from lib.rs:262 to the end of the impl block at line 488 is swallowed into `resolution_cache`'s unterminated body and is therefore never a member of the `#[contractimpl]` block: the entire #690 fee surface (`upsert_fee_tier`, `remove_fee_tier`, `get_fee_tiers`, `set_promotion`, `clear_promotion`, `set_referral_config`, `get_referral_config`, `record_volume`, `get_volume`, `resolve_effective_fee_bps`, `compute_fees`, `is_promotion_active`) and the entire health/rollout surface (`health_check`, `get_health_metrics`, `get_sla_targets`, `set_alert_config`, `get_alert_config`, `detect_anomaly`, `report_ok`, `report_error`, `set_feature_flag`, `is_feature_enabled`, `set_canary_deployment`, `route_to_canary`, `get_rollout_state`, `set_rollback_trigger`, `should_rollback`, `trigger_rollback`), together with their events. (3) Three sibling files have the same defect and are masked because the parser never reaches them: `storage.rs:159-162` (`pub fn remove_resolution_cache` is unterminated, so `get_fee_tiers`, `set_fee_tiers`, `upsert_fee_tier`, `remove_fee_tier`, `get_promotion`, `set_promotion`, `clear_promotion`, `get_referral_config`, `set_referral_config`, `get_volume`, and `record_volume` after it are all textually inside that function); `types.rs:44-46` (`pub struct ResolutionCacheEntry` is unterminated, so `FeeTier`, `Promotion`, `ReferralConfig`, and `FeeBreakdown` are inside it); and `tests.rs:275-283` (`fn registry_entry_returns_full_record` is unterminated). Each of the four files is short exactly one closing brace, so all four breakages must be fixed before the crate will build. (4) `fees.rs` and `errors.rs` are brace-balanced and would compile on their own. Independently of the parse error, `ConfigError` has a duplicate discriminant (`AddressNotRegistered = 6` and `InvalidTier = 6`), which the `#[contracterror]` expansion turns into an unreachable match arm, and lib.rs:16-19 imports `FeeTokenMetadata` and `PlatformConfig` twice from the same `types` module (legal, since both paths resolve to the same item, but redundant).


### rate_limiter

> Rate Limiter Smart Contract — implements configurable rate limiting for account activities to prevent abuse; this module can be used across other contracts to limit specific actions. Closes #710.

Source: `contracts/rate_limiter/`

**Purpose.**

`rate_limiter` is a shared, reusable throttle that other contracts call in-line to decide whether an account may perform an action. It is deliberately generic over three named action classes — `CommissionsPerArtist`, `DisputesPerUser`, `EscrowsPerUser` — each with its own configurable `{limit, window_ledgers}` pair held in a single instance-storage `Map`. Counting is a **fixed window**: the first action of a window records `first_ledger`, and the window rolls over once `window_ledgers` have elapsed. Two design choices are load-bearing and documented in the source: a *rejected* attempt still advances the counter, so a client hammering a closed window cannot keep its position warm; and `check_rate_limit` deliberately takes **no** authorization for the `account` argument, on the stated assumption that the calling contract invokes it inside a transaction where the end user's auth is already present. The crate's own module doc records that it was previously unbuildable — the error type was a `Diagnostic`-carrying struct rather than a `#[contracterror]` enum, `check_rate_limit` built a struct-variant error against it, and `mod test;` pointed at a nonexistent file.

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — **declared but never referenced**; the only mention of `shared` anywhere in `src/` is inside a doc comment at `lib.rs:50` explaining why `initialize_default_config` skips `require_auth`. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with the `testutils` feature as a dev-dependency. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]` — the `rlib` is required (per the Cargo.toml comment) so `src/test.rs` can link. `semver_types.rs` is not a dependency: it is `include!`d from `contracts/semver_types.rs` at `lib.rs:35`.

**Public interface.**

8 externally callable `pub fn`s — the smallest surface of the four. The internal `fn`s `initialize_default_config`, `load_config`, and `all_keys` are outside the block and omitted. This is the only crate of the four that does **not** call `impl_semver_queries!()`; it hand-writes a single `get_version` that calls `parse_pkg_semver` directly, so `get_version_metadata` and `is_version_compatible` are **absent** from its ABI even though the `include!`d file provides them and its `Cargo.toml` declares `min-compatible`/`storage-schema`. The leftover `CURRENT_STORAGE_SCHEMA`, `min_compatible_for`, and `is_compatible` from that include are reported as dead code by the compiler.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), RateLimitError>` | `admin.require_auth()` | `rl_init` | Single-shot: guards on `instance().has(DataKey::Admin)`. Writes `Admin`, then calls the internal `initialize_default_config`, which seeds all three limit classes and publishes `rl_init`. That helper **deliberately does not** re-`require_auth` the admin — asking the host for the same authorization twice in one invocation is a known source of `HostError` |
| `set_limit` | `set_limit(env: Env, admin: Address, key: RateLimitKey, limit: u32, window_ledgers: u32) -> Result<(), RateLimitError>` | `admin.require_auth()` (on the **parameter**) | `rl_set` | The parameter is authorized but **never compared to the stored `DataKey::Admin`** — any address can rewrite any limit class. Validates `limit != 0` (`InvalidLimit`) and `window_ledgers != 0` (`InvalidWindow`) but imposes **no upper bound on `window_ledgers`**. Read-modify-writes the config `Map`; creates the entry if the map is missing |
| `get_limit` | `get_limit(env: Env, key: RateLimitKey) -> Option<RateLimitConfig>` | none (view) | — | `None` for an unconfigured key; falls back to an empty map when `DataKey::RateLimitConfig` is absent entirely (i.e. pre-`initialize`) |
| `check_rate_limit` | `check_rate_limit(env: Env, key: RateLimitKey, account: Address) -> Result<(), RateLimitError>` | **none — no `require_auth` call at all** | `rl_check` on success, `rl_exceeded` on rejection | The core entry point, and the crate's most consequential design decision. No auth is taken for `account` because the calling contract is expected to invoke the check inside a transaction where the end user's auth is already present; a caller can therefore only rate-limit an account by spending that account's own allowance, which the source flags as a griefing vector if a contract ever exposes the check with a caller-chosen `account`. Flow: `NotInitialized` for an unconfigured key; `elapsed = current_ledger - first_ledger`; on rollover (`elapsed > window_ledgers`) the count resets to `1` and `first_ledger` moves to now, otherwise the count is `checked_add`ed (`LimitExceeded` on overflow); the record is **always** written and TTL-bumped, including on rejection; then `record.count > config.limit` ⇒ publish `rl_exceeded` and return `LimitExceeded`. Because the numbers cannot ride in a `#[contracterror]` variant, the limit/window/count triple is carried by the events instead |
| `get_count` | `get_count(env: Env, key: RateLimitKey, account: Address) -> Option<u32>` | none (view) | — | `None` when no record exists; returns the count only, not the window state |
| `reset_limits` | `reset_limits(env: Env, admin: Address, account: Address) -> Result<(), RateLimitError>` | `admin.require_auth()` (on the **parameter**, unchecked against the stored admin) | `rl_reset` | Removes the record for `account` across **all three** classes — the loop is over the hard-coded `all_keys()` array, so a key added to the enum later would be silently skipped. Affects one account only. Payload is the bare `account` value |
| `get_status` | `get_status(env: Env, key: RateLimitKey, account: Address) -> Option<RateLimitRecord>` | none (view) | — | The full record — `count`, `first_ledger`, `last_ledger` — so a client can compute when the window rolls over |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | **Hand-written, not macro-generated.** Calls `parse_pkg_semver(env!("CARGO_PKG_VERSION"))` directly. `get_version_metadata` and `is_version_compatible` do not exist on this contract |

**Events.**

Four shapes, all with a single `symbol_short!`/`Symbol::new` topic.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `rl_init` | `(symbol_short!("rl_init"),)` | `(admin: Address, DEFAULT_COMMISSION_LIMIT: u32, DEFAULT_DISPUTE_LIMIT: u32, DEFAULT_ESCROW_LIMIT: u32)` = `(admin, 5, 3, 10)` | `initialize`, via the internal `initialize_default_config`. Does **not** include `DEFAULT_WINDOW_LEDGERS` |
| `rl_set` | `(symbol_short!("rl_set"),)` | `(key: RateLimitKey, limit: u32, window_ledgers: u32)` | `set_limit` |
| `rl_check` | `(symbol_short!("rl_check"),)` | `(key: RateLimitKey, account: Address, count: u32, limit: u32, window_ledgers: u32)` | `check_rate_limit`, success path |
| `rl_exceeded` | `(Symbol::new(&env, "rl_exceeded"),)` — a `Symbol::new`, **not** `symbol_short!`; the name is 10 characters and exceeds the 9-character `symbol_short!` limit | `(key: RateLimitKey, account: Address, count: u32, limit: u32, window_ledgers: u32)` | `check_rate_limit`, rejection path — published *before* `LimitExceeded` is returned, because a `#[contracterror]` variant cannot carry a payload |

**Errors.**

`errors::RateLimitError`, `#[contracterror]`, unit variants with explicit `= N` discriminants. The module doc states the discriminants are kept stable so already-deployed error codes do not change meaning, and `src/test.rs::error_codes_are_stable` pins all five. This enum replaced a `Diagnostic`-carrying struct that did not exist in `soroban-sdk`.

| Variant | Code | Condition |
|---|---|---|
| `NotInitialized` | 1 | `check_rate_limit` found no `RateLimitConfig` for the requested key — the only construction site, and also the name used when a key is simply unconfigured rather than the contract being uninitialized |
| `AlreadyInitialized` | 2 | `initialize` found `DataKey::Admin` already present in instance storage |
| `InvalidLimit` | 3 | `set_limit` with `limit == 0` — the source notes a zero limit would block every action, including recovery |
| `InvalidWindow` | 4 | `set_limit` with `window_ledgers == 0` — the source notes a zero-length window would expire on the very next ledger |
| `LimitExceeded` | 5 | `check_rate_limit` where `record.count > config.limit`; also returned if `checked_add` on `record.count` overflows |

**Storage.** See [STORAGE.md](./STORAGE.md#rate-limiter).

**Compile status.** `compiles` — `cargo check -p rate_limiter --lib` succeeds, with 4 warnings: three dead-code items spliced in from the `include!`d `contracts/semver_types.rs` (`CURRENT_STORAGE_SCHEMA`, `min_compatible_for`, `is_compatible` — all unused because this crate skips `impl_semver_queries!()`), plus one unused-variable warning in `check_rate_limit`. The 20 unit tests in `src/test.rs` include an error-code stability pin.


### recruitment

> This crate has no `//!` module doc comment. It also has no top-of-file `//` header comment: `src/lib.rs` opens with `#![no_std]`. Nearest `//` comment in the file is the section header at contracts/recruitment/src/lib.rs:480: "── Health monitoring (#678) and gradual rollout (#684) ──". `Cargo.toml` likewise carries no description comment (only the `[package.metadata.stellar-aid]` block).

Source: `contracts/recruitment/`

**Purpose.**

An on-chain hiring pipeline for freelance/creator engagements. An employer `post_job` with a budget and a number of openings; an applicant `apply_for_job` with a proposal URI and a rate; the employer then walks the funnel with `advance_application` (only `Screening` and `Interview` are reachable there) and `make_offer`. Accepting an offer (`accept_offer`) is what hires, incrementing `filled` and flipping the posting to `Filled` once every opening is taken; the applicant can also `decline_offer` or `withdraw_application`, and the employer can `reject_application` at any live stage. Every transition is mirrored into a `Pipeline` of per-stage headcounts (`applied, screening, interview, offered, hired, rejected, withdrawn, declined`) so a funnel can be read without scanning every application, and post-hire `record_performance` accumulates 1–5 star reviews with a free-text note. Identifiers are caller-supplied opaque `Bytes` job ids, and the contract is deliberately off-chain-light: there is no payment or escrow integration here, no offer expiry, and no way to reopen a closed or filled posting. It also re-exports the shared health-monitoring (#678) and gradual-rollout (#684) surface.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — used, via `shared::health` and `shared::rollout`. Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to `21.7.7` in `Cargo.lock`); the dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. It also splices in `contracts/semver_types.rs` via `include!("../../semver_types.rs")` (lib.rs:13). No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, max_applicants: u32) -> Result<(), RecruitmentError>` | **none** — no `require_auth()` in the body | `init` | Guarded by `has_admin` (→ `AlreadyInitialized`) and `max_applicants == 0` (→ `InvalidOpenings`). Writes `Admin` and `MaxApplicants`. Because there is no auth, the first caller of an uninitialised deployment chooses the admin — though that address is never used for authorization afterwards. |
| `get_version` | `fn get_version(_env: Env) -> ContractVersion` (from `impl_semver_queries!()`) | none (view) | — | `parse_pkg_semver(env!("CARGO_PKG_VERSION"))` → `0.1.0`. Exported as a contract entry point? unknown — the macro expands inside `#[contractimpl]`, and the same workspace states in `platform_config/src/lib.rs:262-266` that "SDK-21 does not export macro-generated (`impl_semver_queries!`) functions". |
| `get_version_metadata` | `fn get_version_metadata(env: Env) -> VersionMetadata` (macro) | none (view) | — | Crate name, semver, `min_compatible`, `storage_schema: 1`. Same export caveat. |
| `is_version_compatible` | `fn is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` (macro) | none (view) | — | For `major == 0` requires equal minor and `patch >=` required. Same export caveat. |
| `post_job` | `fn post_job(env: Env, job_id: Bytes, employer: Address, title: String, budget: i128, openings: u32) -> Result<(), RecruitmentError>` | `employer.require_auth()` | `posted` | Requires initialisation. Rejects a duplicate `job_id` (`JobExists`), `budget <= 0` (`InvalidBudget`), and `openings == 0` (`InvalidOpenings`). Creates the `Job` with `status: Open`, `filled: 0`, `applicant_count: 0`, and `posted_ledger = env.ledger().sequence()`. There is no duplicate-content check, so an employer can post the same role under a fresh id. |
| `close_job` | `fn close_job(env: Env, job_id: Bytes) -> Result<(), RecruitmentError>` | `job.employer.require_auth()` — the stored employer, not a passed-in address | `closed` | Requires `status == Open` (else `JobNotOpen`), then sets `status = Closed`. Irreversible: no reopen, and `accept_offer` can still hire off a closed posting only if its own `JobNotOpen` check passes, which it does not — so closing strands any live `Offered` applications. |
| `apply_for_job` | `fn apply_for_job(env: Env, job_id: Bytes, applicant: Address, proposal_uri: String, rate: i128) -> Result<(), RecruitmentError>` | `applicant.require_auth()` | `applied` | Requires initialisation, `status == Open`, and `applicant != job.employer` (`EmployerCannotApply`). Rejects `rate <= 0` (`InvalidBudget`), a duplicate application (`AlreadyApplied`), and `applicant_count >= MaxApplicants` (`TooManyApplicants`). Persists the `Application` (stage `Applied`), appends to `Applicants`, increments `applicant_count`, and moves the pipeline into `Applied`. No proposal deposit or stake is taken. |
| `advance_application` | `fn advance_application(env: Env, job_id: Bytes, applicant: Address, stage: Stage) -> Result<(), RecruitmentError>` | `job.employer.require_auth()` (via the internal `require_employer`) | `advanced` | `require_employer` also loads the application and rejects a terminal one with `InvalidStage`. Only `Stage::Screening` and `Stage::Interview` are accepted, and the target's `rank()` must be strictly greater than the current stage's, so the funnel cannot be run backwards or skipped. `Offered` is deliberately excluded — it has its own two-sided entry point. |
| `make_offer` | `fn make_offer(env: Env, job_id: Bytes, applicant: Address, rate: i128, start_ledger: u32) -> Result<(), RecruitmentError>` | `job.employer.require_auth()` | `offered` | Requires `status == Open`, an application that is not already `Offered` (`InvalidStage`), and `rate <= 0` → `InvalidBudget`. Writes an `Offer { rate, start_ledger, made_ledger }` and moves the stage to `Offered`. `start_ledger` is recorded but never enforced: the offer does not expire, and nothing prevents a later `accept_offer` long after the stated start. |
| `accept_offer` | `fn accept_offer(env: Env, job_id: Bytes, applicant: Address) -> Result<(), RecruitmentError>` | `applicant.require_auth()` | `hired` | Requires the application to be in `Stage::Offered` (else `InvalidStage`). Moves it to `Hired`, increments `job.filled`, and sets `status = Filled` once `filled >= openings`. It does **not** re-check `status == Open`, so an application can be accepted on a closed posting as long as the offer is still standing. The `Offer` record is left in place. |
| `decline_offer` | `fn decline_offer(env: Env, job_id: Bytes, applicant: Address) -> Result<(), RecruitmentError>` | `applicant.require_auth()` | `declined` | Requires stage `Offered` (else `InvalidStage`), then moves to the terminal `Declined`. The employer cannot decline on the applicant's behalf. |
| `withdraw_application` | `fn withdraw_application(env: Env, job_id: Bytes, applicant: Address) -> Result<(), RecruitmentError>` | `applicant.require_auth()` | `withdrawn` | Rejects an already-terminal application (`InvalidStage`), then moves to the terminal `Withdrawn`. The entry point in `DataKey::Applicants` is not removed, so the applicant remains in the submission log. |
| `reject_application` | `fn reject_application(env: Env, job_id: Bytes, applicant: Address) -> Result<(), RecruitmentError>` | `job.employer.require_auth()` (via `require_employer`) | `rejected` | `require_employer` rejects terminal applications, so a rejection is recorded exactly once. There is no rejection reason or note field. |
| `record_performance` | `fn record_performance(env: Env, job_id: Bytes, applicant: Address, rating: u32, note: String) -> Result<(), RecruitmentError>` | `job.employer.require_auth()` | `perf` | Requires rating in `1..=5` (else `InvalidRating`) and an application in `Stage::Hired` (else `NotHired`) — so reviews are only possible after `accept_offer`. Accumulates `reviews` / `total_rating` and overwrites `last_rating`, `last_ledger`, `last_note`. Reviews are append-only in aggregate; there is no way to amend or remove one. |
| `get_job` | `fn get_job(env: Env, job_id: Bytes) -> Result<Job, RecruitmentError>` | none (view) | — | Internal `load_job`; `JobNotFound` when absent. |
| `get_application` | `fn get_application(env: Env, job_id: Bytes, applicant: Address) -> Result<Application, RecruitmentError>` | none (view) | — | Internal `load_application`; `ApplicationNotFound` when absent. |
| `get_offer` | `fn get_offer(env: Env, job_id: Bytes, applicant: Address) -> Option<Offer>` | none (view) | — | `None` when no offer was ever made. |
| `get_applicants` | `fn get_applicants(env: Env, job_id: Bytes) -> Vec<Address>` | none (view) | — | Arrival-ordered applicant list, empty vector when unset. Not paginated, so it returns the whole list. |
| `get_pipeline` | `fn get_pipeline(env: Env, job_id: Bytes) -> Pipeline` | none (view) | — | The per-stage headcounts; an all-zero `Pipeline` for an unknown job. |
| `get_performance` | `fn get_performance(env: Env, job_id: Bytes, applicant: Address) -> Option<Performance>` | none (view) | — | `None` when the applicant has never been reviewed. |
| `get_average_rating` | `fn get_average_rating(env: Env, job_id: Bytes, applicant: Address) -> u32` | none (view) | — | `total_rating * 100 / reviews`, i.e. the mean rating scaled by 100 to keep a hundredth of a star without floating point. `0` when there are no reviews. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt`, `rollback` | Classifies against SLA thresholds and, if `report.anomaly`, calls `shared::rollout::maybe_auto_rollback` — side-effecting despite the name. It is not wired into the recruitment entry points, so nothing records `ok`/`error` samples automatically. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Published SLA constants; `env` is bound and discarded. |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `alrt_cfg` | Auth is on the *passed* `admin`, which is never compared with the stored `DataKey::Admin` — so this and the other `admin`-taking health/rollout setters are authorised by whoever signs, not by the configured admin. `shared::health::set_alert_config` panics `"invalid alert config"` on inverted or out-of-range values. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Stored config, or `default_alert_config()`. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | `true` when degraded, unhealthy, or stalled. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `ok_count`, stamps `last_ok_ledger`. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `error_count`, stamps `last_error_ledger`. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` | `feat_flg` | The kill-switch primitive: writes `Flag(flag)` and indexes the name. No recruitment entry point reads any flag, so flags set here have no effect on this contract. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | The flag value, forced to `false` while the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `canary` | Registers both ids and the traffic share; panics `"canary_bps exceeds 10000"` above the cap. `route_to_canary` is exported but no internal code path calls it, so the split is advisory here. |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky per-caller split — `SHA-256(caller XDR)` mod `10_000` `< canary_bps`; always `false` after a rollback. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | `{ phase, canary_bps, canary, stable, rollback_error_bps, flag_count }`. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `rb_trig` | Arms automatic rollback; panics `"invalid rollback trigger"` on `0` or `> 10_000`. |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | `true` when the health error rate meets the trigger with at least one sample, or the phase is already `RolledBack`. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` | `rollback`, `contract_paused` | Zeroes `canary_bps`, sets `Phase = RolledBack`, forces every indexed flag `false`, and sets `PauseDataKey::Paused = true`. Since no `pause`/`unpause` entry point is exposed and no recruitment function reads the flag, this has no effect on hiring and the flag can never be cleared through this contract. |

Internal (not part of the external interface): module-level private `fn`s `has_admin`, `require_initialized`, `load_job`, `save_job`, `load_application`, `save_application`, `load_pipeline`, `stage_slot`, `shift_pipeline`, `require_employer`, `move_stage`, plus `Stage::rank` and `Stage::is_terminal` on the `#[contracttype]` enum. `require_employer` is the shared guard for the employer-driven transitions: it requires initialisation, loads the job, calls `job.employer.require_auth()`, then loads the application and rejects a terminal stage.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, max_applicants)` | `initialize` |
| `posted` | `[contract, "posted"]` | `(job_id, employer, openings)` | `post_job` |
| `closed` | `[contract, "closed"]` | `job_id` | `close_job` |
| `applied` | `[contract, "applied"]` | `(job_id, applicant)` | `apply_for_job` |
| `advanced` | `[contract, "advanced"]` | `(job_id, applicant, stage)` | `advance_application` |
| `offered` | `[contract, "offered"]` | `(job_id, applicant, rate)` | `make_offer` |
| `hired` | `[contract, "hired"]` | `(job_id, applicant)` | `accept_offer` |
| `declined` | `[contract, "declined"]` | `(job_id, applicant)` | `decline_offer` |
| `withdrawn` | `[contract, "withdrawn"]` | `(job_id, applicant)` | `withdraw_application` |
| `rejected` | `[contract, "rejected"]` | `(job_id, applicant)` | `reject_application` |
| `perf` | `[contract, "perf"]` | `(job_id, applicant, rating)` | `record_performance` |
| `alrt_cfg` | `[contract, "alrt_cfg"]` | `config.unhealthy_error_bps` | `shared::health::set_alert_config` via `set_alert_config` |
| `hlth_alrt` | `[contract, "hlth_alrt"]` | `(report.status, report.error_bps, report.stalled)` | `shared::health::health_check` via `health_check`, only on an anomaly, with alerting enabled, and past the cooldown |
| `canary` | `[contract, "canary"]` | `(canary, stable, canary_bps)` | `shared::rollout::set_canary_deployment` via `set_canary_deployment` |
| `feat_flg` | `[contract, "feat_flg"]` | `(flag, enabled)` | `shared::rollout::set_feature_flag` via `set_feature_flag` |
| `rb_trig` | `[contract, "rb_trig"]` | `error_bps` | `shared::rollout::set_rollback_trigger` via `set_rollback_trigger` |
| `rollback` | `[contract, "rollback"]` | `env.ledger().sequence()` | `shared::rollout::apply_rollback`, from `trigger_rollback` or from `health_check` → `maybe_auto_rollback` |
| `contract_paused` | `[contract, "contract_paused"]` | `shared::pause::ContractPausedEvent { admin }` | `shared::rollout::trigger_rollback` via `trigger_rollback` |

The eleven recruitment events each have exactly one publishing site, so there are no repeats to collapse. The seven `shared` shapes are published by library code in `contracts/shared` that these entry points delegate to, but they land in this contract's own event stream under its contract address.

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` when `DataKey::Admin` already exists. |
| `NotInitialized` | 2 | Any entry point calling the internal `require_initialized` before `DataKey::Admin` exists: `post_job`, `close_job`, `apply_for_job`, `accept_offer`, `decline_offer`, `withdraw_application`, `record_performance`, and `require_employer` (hence also `advance_application`, `make_offer`, `reject_application`). |
| `Unauthorized` | 3 | **Never constructed** in this crate. All authorisation is `require_auth()` on the employer or applicant, so a failed authorisation surfaces as a host `AuthError`, not as this variant. |
| `JobNotFound` | 4 | `load_job` when `DataKey::Job(job_id)` is absent: `close_job`, `apply_for_job`, `accept_offer`, `record_performance`, `get_job`, and `require_employer`. |
| `JobExists` | 5 | `post_job` when the caller-supplied `job_id` is already taken. |
| `JobNotOpen` | 6 | `status != JobStatus::Open` in `close_job` (i.e. closing a closed or filled posting), `apply_for_job`, or `make_offer`. |
| `ApplicationNotFound` | 7 | `load_application` when no `Application(job_id, applicant)` exists: `accept_offer`, `decline_offer`, `withdraw_application`, `record_performance`, `get_application`, and `require_employer`. |
| `AlreadyApplied` | 8 | `apply_for_job` when the applicant has already applied to that job id. |
| `InvalidStage` | 9 | Terminal stage where a live one is required (`require_employer`, `withdraw_application`); target not `Screening`/`Interview` or `rank()` not strictly increasing (`advance_application`); not `Offered` (`accept_offer`, `decline_offer`); already `Offered` (`make_offer`); either stage lacking a `rank()` (`advance_application`). |
| `InvalidRating` | 10 | `record_performance` with `rating` outside `1..=5`. |
| `InvalidBudget` | 11 | `post_job` with `budget <= 0`; `apply_for_job` or `make_offer` with `rate <= 0`. |
| `InvalidOpenings` | 12 | `initialize` with `max_applicants == 0`; `post_job` with `openings == 0`. |
| `EmployerCannotApply` | 13 | `apply_for_job` where `job.employer == applicant`. |
| `TooManyApplicants` | 14 | `apply_for_job` when `job.applicant_count >= MaxApplicants` (which is `0` if the key was never set, blocking applications entirely). |
| `NotHired` | 15 | `record_performance` where the application is not in `Stage::Hired`. |

The enum is a `#[contracterror]` with explicit discriminants and is the sole failure channel; the crate contains no `panic!`, `assert!`, or `expect` of its own. `errors.rs` also exposes a non-contract helper `get_suggestion(RecruitmentError) -> Symbol` (suggestions `DUP`, `NO_INIT`, `AUTH`, `NO_JOB`, `JOB_DUP`, `CLOSED`, `NO_APP`, `APP_DUP`, `BAD_STAGE`, `BAD_RATE`, `BAD_BUDG`, `BAD_OPEN`, `SELF_APP`, `TOO_MANY`, `NOT_HIRED`), which is reachable as a Rust item but is not a contract entry point.

**Storage.** See [STORAGE.md](./STORAGE.md#recruitment).

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p recruitment` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The `#[cfg(test)] mod test` module (contracts/recruitment/src/test.rs, 371 lines) is brace- and paren-balanced and compiles under `cargo test`. Behavioural notes that do not affect compilation: the `admin` written by `initialize` is never used for authorization, so the `admin`-taking health/rollback setters are gated on whoever signs rather than on the configured admin; and the `admin`-parameterised `set_alert_config` / `report_ok` / `report_error` / `set_feature_flag` / `set_canary_deployment` / `set_rollback_trigger` / `trigger_rollback` are reachable by any address that signs.


### reputation

> Review Moderation & Appeal System — review submission with rating and comment, review reporting, admin moderation queue and decision tracking, an appeal mechanism for artists and clients, and escalation to the dispute arbiter. Closes #604.

Source: `contracts/reputation/`

**Purpose.**

As written, `reputation` is two different contracts occupying one file. The first fragment (`#[contract] pub struct ReputationContract`, lines 1–343) is a moderation and appeals system: clients submit rated reviews keyed by a caller-chosen `review_id`, anyone reports a review once with a `ReportReason`, reports push the review into an instance-stored admin queue and flip it to `UnderReview`, an admin moderates to `Removed`/`Cleared` while appending to a per-review `ModerationRecord` history, and the affected artist or the original reviewer may file one appeal and escalate it. The second fragment (`#[contract] pub struct Reputation`, spliced in from line 344) is a rating/reputation-scoring system for #597: one review per `(artist, client)` pair, artist-initiated disputes, a moderator role set, and a recency-weighted, confidence-adjusted 0–100 score. Neither fragment compiles as part of the file, so the effective on-chain contract is **unknown — the source contains a parse error before any semantic analysis; the two fragments declare conflicting `DataKey`, `ReviewStatus`, `ReputationError`, and duplicate `initialize` entry points.** Both `src/tests.rs` (targeting `ReputationContractClient`, `rating_x10` 10–50) and `src/test.rs` (targeting `ReputationClient`, `rating` 1–5, `get_reputation`) are wired up, confirming the duplication. `shared` is declared as a path dependency in `Cargo.toml` but no fragment references it.

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — declared but **unused by any fragment**, so it is dead weight today. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with `testutils` as a dev-dependency. `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. The second fragment `include!`s `../../semver_types.rs`, so its three semver queries read only `Cargo.toml`.

**Public interface.**

Listed by fragment, with the `Authorization` column read from the source as written. Where a function is truncated mid-body it is flagged, because its real post-truncation behaviour cannot be determined.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), ReputationError>` (A, line 41) | `admin.require_auth()` | `init` | `instance().has(Admin)` checked before `require_auth()`; `AlreadyInitialized` on the second call |
| `submit_review` | `submit_review(env: Env, review_id: Bytes, artist: Address, reviewer: Address, rating_x10: u32, comment: String) -> Result<(), ReputationError>` (A, line 57) | `reviewer.require_auth()` | `review` | `rating_x10` must be `10..=50` else `InvalidRating`; a duplicate `review_id` is silently rejected as `AlreadyReported` (no comment-length check in this fragment) |
| `get_review` | `get_review(env: Env, review_id: Bytes) -> Result<ReviewRecord, ReputationError>` (A, line 99) | none (view) | — | `ReviewNotFound` |
| `report_review` | `report_review(env: Env, review_id: Bytes, reporter: Address, reason: ReportReason, details: String) -> Result<(), ReputationError>` (A, line 115) | `reporter.require_auth()` | `reported` | Only `Active` or `Cleared` reviews may be reported; linear dedup scan over `0..ReportCount`; sets status `UnderReview` and appends to the instance queue |
| `get_moderation_queue_size` | `get_moderation_queue_size(env: Env) -> u32` (A, line 190) | none (view) | — | Infallible |
| `get_queue_entry` | `get_queue_entry(env: Env, index: u32) -> Result<Bytes, ReputationError>` (A, line 195) | none (view) | — | Returns the queued `review_id`; `ReviewNotFound` when the slot is empty |
| `moderate_review` | `moderate_review(env: Env, review_id: Bytes, new_status: ReviewStatus, notes: String) -> Result<(), ReputationError>` (A, line 208) | `admin.require_auth()` | `moderated` | Requires current status `UnderReview` and target `Removed` or `Cleared`, else `InvalidStatus`; appends a `ModerationRecord`; despite the doc comment, does not remove anything from the queue |
| `get_moderation_entry` | `get_moderation_entry(env: Env, review_id: Bytes, index: u32) -> Result<ModerationRecord, ReputationError>` (A, line 274) | none (view) | — | |
| `get_moderation_count` | `get_moderation_count(env: Env, review_id: Bytes) -> u32` (A, line 286) | none (view) | — | Infallible |
| `file_appeal` | `file_appeal(env: Env, review_id: Bytes, appellant: Address, reason: String) -> Result<(), ReputationError>` (A, line 301) | `appellant.require_auth()` | `appeal` — **publish is unterminated** | Guard order: `Removed`-only, then appellant must be the artist or reviewer (`Unauthorized`), then no existing appeal (`AppealAlreadyExists`). Body is cut off inside the `env.events().publish((symbol_short!("appeal"),), (review_id, appellant),` call at line 341, so the closing `)`/`;`/`Ok(())` are missing — the post-truncation behaviour is `unknown` |
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), ReputationError>` (B, line 478) | **none — no `require_auth()` call** | `init` | Conflicts with fragment A's `initialize`; payload is a bare `admin`, not a tuple |
| `get_version` | `get_version(env: Env) -> ContractVersion` (B, from `impl_semver_queries!()`, line 487) | none (view) | — | Cargo.toml-only; no storage read |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` (B, macro) | none (view) | — | |
| `is_version_compatible` | `is_version_compatible(env: Env, major: u32, minor: u32, patch: u32) -> bool` (B, macro) | none (view) | — | |
| `add_moderator` | `add_moderator(env: Env, moderator: Address) -> Result<(), ReputationError>` (B, line 489) | `admin.require_auth()` (via `require_admin`) | `mod_add` | |
| `remove_moderator` | `remove_moderator(env: Env, moderator: Address) -> Result<(), ReputationError>` (B, line 499) | `admin.require_auth()` | `mod_rm` | `instance().remove(Moderator(..))` — the only `remove` in the crate |
| `is_moderator` | `is_moderator(env: Env, moderator: Address) -> bool` (B, line 508) | none (view) | — | Does not fall back to "is admin", unlike `require_moderator` |
| `submit_review` | `submit_review(env: Env, client: Address, artist: Address, rating: u32, comment: String) -> Result<u32, ReputationError>` (B, line 517) | `client.require_auth()` | `rev_new` | Different signature from fragment A's: `rating` is `1..=5`, order is `(client, artist, …)`, and it returns the new index. Requires initialization, then rating range, then `MAX_COMMENT_LEN`, then the `HasReviewed` dedup guard (`DuplicateReview`) |
| `dispute_review` | `dispute_review(env: Env, artist: Address, review_index: u32, reason: String) -> Result<(), ReputationError>` (B, line 566) | `artist.require_auth()` | `rev_disp` | Only the reviewed artist may dispute, and only from `Active`; sets `Disputed` + `dispute_reason`, excluding it from the score |
| `resolve_appeal` | `resolve_appeal(env: Env, review_id: Bytes, decision: AppealStatus) -> Result<(), ReputationError>` (B, line 608) | `admin.require_auth()` | `appeal_rs` — **publish is unterminated** | Reads fragment A's `DataKey::Appeal` while living in fragment B's impl; requires `Pending` and rejects `decision == Pending`; on `Upheld` reinstates `DataKey::Review` to `Active`. Body is cut off inside the `env.events().publish((symbol_short!("appeal_rs"),), (review_id, admin),` call at line 644, so it never closes and the remaining text of fragment B is swallowed into it |
| `resolve_dispute` | `resolve_dispute(env: Env, moderator: Address, artist: Address, review_index: u32, uphold_review: bool, note: String) -> Result<(), ReputationError>` (B, line 647) | `moderator.require_auth()`, plus the moderator must be the admin or in `DataKey::Moderator`, else `Unauthorized` (`require_moderator`) | `rev_res` | Textually **nested inside the unterminated `resolve_appeal` body** (it begins at line 645 before that function's `publish` call is closed), so it is not a real item of the impl block in the file as written. Intended behaviour: `Disputed` → `Upheld` (counts again) or `Removed` |
| `get_appeal` | `get_appeal(env: Env, review_id: Bytes) -> Result<AppealRecord, ReputationError>` (B, line 686) | none (view) | — | `AppealNotFound` |
| `get_report_count` | `get_report_count(env: Env, review_id: Bytes) -> u32` (B, line 694) | none (view) | — | Infallible |
| `get_report` | `get_report(env: Env, review_id: Bytes, index: u32) -> Result<ReportRecord, ReputationError>` (B, line 702) | none (view) | — | Returns `ReviewNotFound` (not a report-specific error) when absent |
| `escalate_appeal` | `escalate_appeal(env: Env, review_id: Bytes, appellant: Address) -> Result<(), ReputationError>` (B, line 719) | `appellant.require_auth()`, and `appeal.appellant` must match, else `Unauthorized` | `escalated` | Requires `Pending`; only records the escalation on chain — the external dispute arbiter is invoked separately by its own admin |
| `moderate_review` | `moderate_review(env: Env, moderator: Address, artist: Address, review_index: u32, note: String) -> Result<(), ReputationError>` (B, line 757) | `moderator.require_auth()` + moderator role (`require_moderator`) | `rev_mod` | Textually **nested inside the unterminated `require_admin` helper** (which begins at line 750 and is spliced mid-body at line 755), so it is not a real item of the impl block. Direct moderation of a non-`Removed` review to `Removed`; conflicts by name and arity with fragment A's `moderate_review` |
| `get_reviews` | `get_reviews(env: Env, artist: Address) -> Vec<Review>` (B, line 786) | none (view) | — | Returns the whole vector unbounded — `unknown — no page or length cap is applied, unlike `audit`'s reads` |
| `get_review` | `get_review(env: Env, artist: Address, review_index: u32) -> Result<Review, ReputationError>` (B, line 790) | none (view) | — | Different signature from fragment A's `get_review`; indexes into the artist's vector |
| `has_reviewed` | `has_reviewed(env: Env, artist: Address, client: Address) -> bool` (B, line 796) | none (view) | — | Reads `HasReviewed` |
| `get_reputation` | `get_reputation(env: Env, artist: Address) -> u32` (B, line 803) | none (view) | — | Cached score; `0` when unset |
| `get_review_count` | `get_review_count(env: Env, artist: Address) -> u32` (B, line 810) | none (view) | — | `Vec::len()` |

**Events.**

The two fragments both define an `init` shape with different payload types, and both define a `moderated`/`rev_mod` pair, so these cannot be deduplicated across fragments.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `(admin,)` | `initialize` (A, line 47) |
| `init` | `(symbol_short!("init"),)` | `admin` (bare `Address`) | `initialize` (B, line 483) |
| `review` | `(symbol_short!("review"),)` | `(review_id: Bytes, artist: Address, reviewer: Address, rating_x10: u32)` — `comment` is not published | `submit_review` (A, line 91) |
| `reported` | `(symbol_short!("reported"),)` | `(review_id, reporter)` | `report_review` (A, line 178) |
| `moderated` | `(symbol_short!("moderated"),)` | `(review_id, admin)` | `moderate_review` (A, line 264) |
| `appeal` | `(symbol_short!("appeal"),)` | `(review_id, appellant)` — **the publish call is unterminated at line 341** | `file_appeal` (A) |
| `mod_add` | `(symbol_short!("mod_add"),)` | `moderator` (bare `Address`) | `add_moderator` (B, line 494) |
| `mod_rm` | `(symbol_short!("mod_rm"),)` | `moderator` (bare `Address`) | `remove_moderator` (B, line 504) |
| `rev_new` | `(symbol_short!("rev_new"),)` | `(artist, client, rating, index, score)` | `submit_review` (B, line 557) |
| `rev_disp` | `(symbol_short!("rev_disp"),)` | `(artist, review_index, score)` | `dispute_review` (B, line 595) |
| `appeal_rs` | `(symbol_short!("appeal_rs"),)` | `(review_id, admin)` — **the publish call is unterminated at line 644** | `resolve_appeal` (B) |
| `rev_res` | `(symbol_short!("rev_res"),)` | `(artist, review_index, uphold_review, score)` | `resolve_dispute` (B, line 676) |
| `rev_mod` | `(symbol_short!("rev_mod"),)` | `(artist, review_index, score)` | `moderate_review` (B, line 781) |
| `escalated` | `(symbol_short!("escalated"),)` | `(review_id, appellant)` | `escalate_appeal` (B, line 741) |

**Errors.**

`errors::ReputationError`, `#[contracterror]`, `#[repr(u32)]`. The enum is itself broken: it declares 14 variants of which four repeat an earlier name, so its final variant set and code assignment are `unknown — duplicate discriminants (5, 6, 7, 8) and duplicate variant names make the enum unrepresentable as written`. Rows below give each declaration in source order.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | A `DataKey::Admin` was already present in instance storage |
| `NotInitialized` | 2 | `require_admin`/`get_admin` found no admin, or a `has_admin` guard failed before a write |
| `Unauthorized` | 3 | `file_appeal` appellant is neither the reviewed artist nor the reviewer; `resolve_dispute`/`moderate_review` caller is not the admin and not a registered moderator; `dispute_review` on a review belonging to another artist; `escalate_appeal` where the appellant is not the appeal's own appellant |
| `ReviewNotFound` | 4 | No review record for the `review_id`, or the artist-vector index is out of range, or a `get_report` index is absent |
| `AlreadyReported` | 5 | A duplicate `review_id` on `submit_review` (A), or the same reporter already reported that review. **Collides with `DuplicateReview = 5`** |
| `InvalidRating` | 6 | `rating_x10` outside `10..=50` (A) or `rating` outside `1..=5` (B). **Collides with the second `ReviewNotFound = 6`** |
| `InvalidStatus` | 7 | Operation not permitted in the current review or appeal status — reporting a non-`Active`/`Cleared` review, moderating something not `UnderReview`, targeting a status other than `Removed`/`Cleared`, appealing a non-`Removed` review, disputing a non-`Active` review, deciding a non-`Pending` appeal, choosing `Pending` as the decision, escalating a non-`Pending` appeal, resolving a non-`Disputed` review, or moderating an already-`Removed` review. **Declared twice (`= 7` and `= 7`)** |
| `AppealAlreadyExists` | 8 | `file_appeal` found an existing `DataKey::Appeal` for the review. **Collides with `CommentTooLong = 8`** |
| `AppealNotFound` | 9 | `get_appeal`, `resolve_appeal`, `escalate_appeal` found no appeal for the `review_id` |
| `ArithmeticOverflow` | 10 | **Never constructed** in either fragment — no `checked_add` survives in the moderation code. Mapped in the first `Display` impl and first `get_suggestion` only |
| `DuplicateReview` | 5 | (B) the client has already reviewed this artist, per the `HasReviewed` guard. **Duplicate discriminant with `AlreadyReported = 5`** |
| `ReviewNotFound` (second declaration) | 6 | **Duplicate name and duplicate discriminant** — the declaration itself is illegal |
| `InvalidStatus` (second declaration) | 7 | **Duplicate name and duplicate discriminant** — the declaration itself is illegal |
| `CommentTooLong` | 8 | `require_comment_len` found a comment, dispute reason, or moderation note longer than `MAX_COMMENT_LEN` (512 bytes). **Duplicate discriminant with `AppealAlreadyExists = 8`** |

`errors.rs` additionally declares `get_suggestion` twice (lines 41 and 65) and splices the tail of a second `Display` `match` arm list into the middle of the first `get_suggestion` at line 53, so the file has three brace-level defects of its own independent of `lib.rs`.

> **Compile status:** **does not compile.** `cargo check -p reputation --lib` fails with exactly one reported error, a *parse* error: `error: this file contains an unclosed delimiter --> contracts/reputation/src/lib.rs:813:3`, with rustc pointing at five unclosed delimiters — `impl ReputationContract` (line 35), the `file_appeal` return type (line 306), the `file_appeal` `env.events().publish(` call (line 341), `impl Reputation` (line 477), and the `require_admin` return type (line 750) — and noting that the final `}` at line 813 "matches this but it has different indentation". `lib.rs` is two contracts spliced together: fragment A (`ReputationContract`, moderation/appeals) is cut off inside the `file_appeal` event publish at line 341, and fragment B (`Reputation`) begins at line 344 with a second crate-level `//!` doc comment and a second `#![no_std]` inner attribute in mid-file, is cut off inside the `resolve_appeal` event publish at line 644 (so `resolve_dispute` and, later, `moderate_review` are nested inside other functions' bodies), and ends with a `require_admin` helper whose body is itself spliced mid-expression at line 755. Because the parse fails, rustc performs no name resolution or type checking, so the remaining errors are latent and **not enumerable without first fixing the delimiter**; those visible directly in the source are: duplicate `DataKey` and duplicate `ReviewStatus` enums in `types.rs` (the first `DataKey` is itself unterminated at line 118), duplicate `initialize` entry points and duplicate `submit_review`/`get_review`/`moderate_review` names across the two fragments, two `get_suggestion` definitions plus a spliced `Display` arm list in `errors.rs`, and the duplicate variant names and discriminants in `ReputationError` listed above. Both test modules are wired up (`#[cfg(test)] mod tests;` at line 18, `#[cfg(test)] mod test;` at line 359) and target different contracts, so neither is the surviving one. All documentation of the fragment-B rows above describes the code as *written*, not as a working contract; the effective ABI, storage layout, and error codes are `unknown — the crate does not parse`.

**Storage.** See [STORAGE.md](./STORAGE.md#reputation).

**Compile status.** `does not compile` — unclosed delimiter. `contracts/reputation/src/lib.rs` contains **two** `#[contract]` types (`ReputationContract` at line 1 and `Reputation`, spliced in at line 344) whose `DataKey`, `ReviewStatus` and `ReputationError` definitions collide, and both declare an `initialize` entry point. Only one of the two can survive a fix, so which storage keys and error codes a deployed `reputation` would expose is **unknown — the crate does not parse, so no semantic analysis is possible**. Both `src/tests.rs` (targeting `ReputationContractClient`, `rating_x10` 10–50) and `src/test.rs` (targeting `ReputationClient`, `rating` 1–5, `get_reputation`) are wired up, confirming the duplication. `ReputationError` also has duplicate names and duplicate discriminants (5, 6, 7, 8) within a single fragment, so its final variant set and code assignment are likewise undetermined.


### revenue_sharing

> This crate has no `//!` module doc comment. It also has no top-of-file `//` header comment: `src/lib.rs` opens with `#![no_std]`. Nearest `//` comment in the file is the section header at contracts/revenue_sharing/src/lib.rs:326: "── Health monitoring (#678) and gradual rollout (#684) ──". `Cargo.toml` likewise carries no description comment. The crate-level context for this design lives in docs/ADRs/0005-platform-fee-and-revenue-distribution.md.

Source: `contracts/revenue_sharing/`

**Purpose.**

Multi-party revenue distribution on a single SEP-41 token, paired with `platform_config` under ADR-0005. An `owner` creates an `Agreement` with a `Vec<Participant>` of `(account, share_bps)` splits, which `validate_splits` forces to cover exactly `TOTAL_BPS` (10,000 bps = 100%) with no zero shares, no duplicates, and at most `MAX_PARTICIPANTS` (20) participants — so a payout can neither leak value nor pay an account twice. `record_revenue` is the single call that books revenue *and* settles it: it floors each share to whole token units, gives the rounding dust to the first participant so the distributed total always equals the gross, writes the attribution and totals first (checks-effects-interactions), then pulls each payout from the `source` address with `token::Client::transfer`. Terms can be replaced mid-flight via `update_splits`, which bumps `terms_version` so already-booked revenue keeps its old attribution while future distributions follow the new terms; each `RevenueEntry` in the bounded history records the `terms_version` in force when it was booked. The owner can `set_status` (`Active` / `Paused` / `Terminated`, with `Terminated` absorbing) and post-hire style reporting is available via `get_earnings`, `get_history`, and `get_report`. It also re-exports the shared health-monitoring (#678) and gradual-rollout (#684) surface.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — used, via `shared::health` and `shared::rollout`. Registry deps: `soroban-sdk.workspace = true` → `21.0.0` (resolved to `21.7.7` in `Cargo.lock`); the dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. It also splices in `contracts/semver_types.rs` via `include!("../../semver_types.rs")` (lib.rs:16). No other dependencies.

**Public interface.**

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, history_limit: u32) -> Result<(), RevenueError>` | **none** — no `require_auth()` in the body | `init` | Guarded by `has_admin` (→ `AlreadyInitialized`) and `history_limit == 0` (→ `InvalidAmount`). Writes `Admin` and `HistoryLimit`. Because there is no auth, the first caller of an uninitialised deployment sets both — though the `admin` value is never used for authorization afterwards. |
| `get_version` | `fn get_version(_env: Env) -> ContractVersion` (from `impl_semver_queries!()`) | none (view) | — | `parse_pkg_semver(env!("CARGO_PKG_VERSION"))` → `0.1.0`. Exported as a contract entry point? unknown — the macro expands inside `#[contractimpl]`, and the same workspace states in `platform_config/src/lib.rs:262-266` that "SDK-21 does not export macro-generated (`impl_semver_queries!`) functions". |
| `get_version_metadata` | `fn get_version_metadata(env: Env) -> VersionMetadata` (macro) | none (view) | — | Crate name, semver, `min_compatible`, `storage_schema: 1`. Same export caveat. |
| `is_version_compatible` | `fn is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` (macro) | none (view) | — | For `major == 0` requires equal minor and `patch >=` required. Same export caveat. |
| `create_agreement` | `fn create_agreement(env: Env, id: Bytes, owner: Address, token: Address, participants: Vec<Participant>) -> Result<(), RevenueError>` | `owner.require_auth()` | `created` | Requires initialisation. Rejects a duplicate `id` (`AgreementExists`) and runs `validate_splits` (non-empty, ≤ `MAX_PARTICIPANTS`, no `share_bps == 0`, no duplicate account, `total == TOTAL_BPS` → `EmptySplit` / `TooManyParticipants` / `InvalidSplitTotal` / `DuplicateParticipant`, with a checked add mapped to `ArithmeticOverflow`). The `token` address is not checked for code existence — an agreement can be created against an address that is not a SEP-41 contract, and the failure then surfaces only at `record_revenue`. |
| `update_splits` | `fn update_splits(env: Env, id: Bytes, participants: Vec<Participant>) -> Result<u32, RevenueError>` | `agreement.owner.require_auth()` | `terms` | Rejects a `Terminated` agreement (`AgreementNotActive`), re-validates the new splits, then increments `terms_version`, stamps `updated_ledger`, and wholly replaces `Splits(id)`. Already-booked `RevenueEntry`s and `Earnings` keep their old attribution, as documented. Returns the new `terms_version`. No diff/limit on how often terms may change. |
| `set_status` | `fn set_status(env: Env, id: Bytes, status: AgreementStatus) -> Result<(), RevenueError>` | `agreement.owner.require_auth()` | `status` | Rejects a `Terminated` agreement, so termination is final. Any other status may be set in any order, including `Active → Active`. `Paused` blocks only `record_revenue` (which requires `Active`); it does not freeze reads or the ability to change terms. |
| `record_revenue` | `fn record_revenue(env: Env, id: Bytes, source: Address, amount: i128, memo: String) -> Result<i128, RevenueError>` | `source.require_auth()` | `revenue` | The core settlement call. Rejects `amount <= 0` (`InvalidAmount`) and `status != Active` (`AgreementNotActive`), re-runs `validate_splits` on the stored splits, then for each participant computes `amount * share_bps / 10_000` (checked-mul → `ArithmeticOverflow`), adds the positive dust to participant 0 so the payouts sum to `amount` exactly, writes all `Earnings` and the agreement totals and the history entry **before** any token movement, and finally calls `token::Client::transfer(&source, &participant.account, &payout)` for each non-zero payout. Those transfers pull from `source`, so `source` must have approved this contract in the token contract — this contract has no `approve`/`allowance` entry point of its own, and a revert in any single transfer reverts the whole call atomically. Returns `amount`. The `memo` is stored verbatim in history and is otherwise unconstrained. |
| `get_agreement` | `fn get_agreement(env: Env, id: Bytes) -> Result<Agreement, RevenueError>` | none (view) | — | Internal `load_agreement`; `AgreementNotFound` when absent. |
| `get_splits` | `fn get_splits(env: Env, id: Bytes) -> Vec<Participant>` | none (view) | — | Current terms; an empty vector for an unknown id (the `unwrap_or_else(Vec::new)` in `load_splits` makes "no agreement" and "no splits" indistinguishable here, unlike `get_agreement`). |
| `get_earnings` | `fn get_earnings(env: Env, id: Bytes, account: Address) -> i128` | none (view) | — | Lifetime amount attributed to `account`, `0` when never paid. |
| `get_history` | `fn get_history(env: Env, id: Bytes) -> Vec<RevenueEntry>` | none (view) | — | The retained (trimmed) history buffer, oldest first. Not paginated, so it returns the whole buffer, bounded by `HistoryLimit`. |
| `get_report` | `fn get_report(env: Env, id: Bytes) -> Result<RevenueReport, RevenueError>` | none (view) | — | Aggregate view: `{ total_revenue, total_distributed, entry_count, terms_version, status }`; `AgreementNotFound` when absent. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view) | `hlth_alrt`, `rollback` | Classifies against SLA thresholds and, if `report.anomaly`, calls `shared::rollout::maybe_auto_rollback`. Side-effecting despite the name, and nothing in the revenue flow records `ok`/`error` samples automatically. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Published SLA constants; `env` is bound and discarded. |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` | `alrt_cfg` | Auth is on the *passed* `admin`, never compared with the stored `DataKey::Admin`, so this and the other `admin`-taking health/rollout setters are authorised by whoever signs. `shared::health::set_alert_config` panics `"invalid alert config"` on inverted or out-of-range values. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Stored config, or `default_alert_config()`. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | `true` when degraded, unhealthy, or stalled. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `ok_count`, stamps `last_ok_ledger`. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` | — | Increments `error_count`, stamps `last_error_ledger`. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: Symbol, enabled: bool)` | `admin.require_auth()` | `feat_flg` | Writes `Flag(flag)` and indexes the name. No revenue entry point reads any flag, so flags set here have no effect on this contract. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: Symbol) -> bool` | none (view) | — | The flag value, forced to `false` while the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` | `canary` | Registers both ids and the traffic share; panics `"canary_bps exceeds 10000"` above the cap. `route_to_canary` is exported but no internal path calls it, so the split is advisory here. |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky per-caller split — `SHA-256(caller XDR)` mod `10_000` `< canary_bps`; always `false` after a rollback. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | `{ phase, canary_bps, canary, stable, rollback_error_bps, flag_count }`. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` | `rb_trig` | Arms automatic rollback; panics `"invalid rollback trigger"` on `0` or `> 10_000`. |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | `true` when the health error rate meets the trigger with at least one sample, or the phase is already `RolledBack`. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` | `rollback`, `contract_paused` | Zeroes `canary_bps`, sets `Phase = RolledBack`, forces every indexed flag `false`, and sets `PauseDataKey::Paused = true`. Since this contract reads no pause flag and exposes no unpause path, a rollback has no effect on revenue sharing and the flag can never be cleared through this contract. |

Internal (not part of the external interface): module-level private `fn`s `has_admin`, `require_initialized`, `load_agreement`, `save_agreement`, `load_splits`, `validate_splits`, `add_earnings`, and `push_history`. `validate_splits` is the invariant guard invoked both when terms are written and again on every `record_revenue`; `push_history` is the bounded-append helper.

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple passed to `publish`.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, history_limit)` | `initialize` |
| `created` | `[contract, "created"]` | `(id, owner)` | `create_agreement` |
| `terms` | `[contract, "terms"]` | `(id, agreement.terms_version)` | `update_splits` — the new version, not the old |
| `status` | `[contract, "status"]` | `(id, status)` | `set_status` — the status just written |
| `revenue` | `[contract, "revenue"]` | `(id, source, amount)` | `record_revenue` |
| `alrt_cfg` | `[contract, "alrt_cfg"]` | `config.unhealthy_error_bps` | `shared::health::set_alert_config` via `set_alert_config` |
| `hlth_alrt` | `[contract, "hlth_alrt"]` | `(report.status, report.error_bps, report.stalled)` | `shared::health::health_check` via `health_check`, only on an anomaly, with alerting enabled, and past the cooldown |
| `canary` | `[contract, "canary"]` | `(canary, stable, canary_bps)` | `shared::rollout::set_canary_deployment` via `set_canary_deployment` |
| `feat_flg` | `[contract, "feat_flg"]` | `(flag, enabled)` | `shared::rollout::set_feature_flag` via `set_feature_flag` |
| `rb_trig` | `[contract, "rb_trig"]` | `error_bps` | `shared::rollout::set_rollback_trigger` via `set_rollback_trigger` |
| `rollback` | `[contract, "rollback"]` | `env.ledger().sequence()` | `shared::rollout::apply_rollback`, from `trigger_rollback` or from `health_check` → `maybe_auto_rollback` |
| `contract_paused` | `[contract, "contract_paused"]` | `shared::pause::ContractPausedEvent { admin }` | `shared::rollout::trigger_rollback` via `trigger_rollback` |

The five revenue events each have exactly one publishing site, so there are no repeats to collapse. The seven `shared` shapes are published by library code in `contracts/shared` that these entry points delegate to, but they land in this contract's own event stream under its contract address. The SEP-41 `transfer` calls made during `record_revenue` emit no event here — those events originate from the token contract.

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` when `DataKey::Admin` already exists. |
| `NotInitialized` | 2 | Any entry point calling the internal `require_initialized` before `DataKey::Admin` exists: `create_agreement`, `update_splits`, `set_status`, `record_revenue`. |
| `Unauthorized` | 3 | **Never constructed** in this crate. All authorisation is `require_auth()` on the agreement `owner` or on `source`, so a failed authorisation surfaces as a host `AuthError`, not as this variant. |
| `AgreementNotFound` | 4 | `load_agreement` when `DataKey::Agreement(id)` is absent: `update_splits`, `set_status`, `record_revenue`, `get_agreement`, `get_report`. |
| `AgreementExists` | 5 | `create_agreement` when the caller-supplied `id` is already taken. |
| `EmptySplit` | 6 | `validate_splits` with an empty participant list — from `create_agreement`, `update_splits`, or `record_revenue` re-validating the stored splits. |
| `TooManyParticipants` | 7 | `validate_splits` with more than `MAX_PARTICIPANTS` (20) participants. |
| `InvalidSplitTotal` | 8 | `validate_splits` when any `share_bps == 0` or the shares do not total `TOTAL_BPS` (10,000). |
| `DuplicateParticipant` | 9 | `validate_splits` when the same `account` appears more than once (checked pairwise against later entries). |
| `InvalidAmount` | 10 | `record_revenue` with `amount <= 0`; also `initialize` with `history_limit == 0` (the mismatched reuse of an amount error for a history bound). |
| `AgreementNotActive` | 11 | `record_revenue` when `status != AgreementStatus::Active` (so `Paused` and `Terminated` both block it); `update_splits` and `set_status` when the agreement is already `Terminated`. |
| `ArithmeticOverflow` | 12 | `validate_splits` on a `checked_add` of `share_bps`; `record_revenue` on a `checked_mul` of `amount * share_bps` or a `checked_add` of `total_revenue` / `total_distributed`. |

The enum is a `#[contracterror]` with explicit discriminants and is the sole failure channel; the crate contains no `panic!`, `assert!`, or `expect` of its own. `errors.rs` also exposes a non-contract helper `get_suggestion(RevenueError) -> Symbol` (suggestions `DUP`, `NO_INIT`, `AUTH`, `NOT_FOUND`, `EXISTS`, `NO_SPLIT`, `TOO_MANY`, `BAD_BPS`, `DUP_PART`, `BAD_AMT`, `INACTIVE`, `OVERFL`), reachable as a Rust item but not a contract entry point.

**Storage.** See [STORAGE.md](./STORAGE.md#revenue-sharing).

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p revenue_sharing` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The `#[cfg(test)] mod test` module (contracts/revenue_sharing/src/test.rs, 300 lines) is brace- and paren-balanced and compiles under `cargo test`. Behavioural notes that do not affect compilation: the `admin` written by `initialize` is never used for authorization, so the `admin`-taking health/rollback setters are gated on whoever signs rather than on the configured admin; and the crate does not call `shared::pause` at all, contradicting `contracts/shared/src/pause.rs:10` which names `revenue_sharing` as one of the five `pause`/`unpause` consumers — so a `trigger_rollback` pause flag is set here but can neither be honoured nor cleared.


### search

> Search contract — indexes artists for discovery with filtering, sorting, pagination, keyword ("full-text") metadata, and search analytics (#599).

Source: `contracts/search/`

**Purpose.**

`search` is the platform's artist-discovery index. Artists maintain their own listing — skill tags, a price, and free-form keyword tags — and clients query the indexed set with exact-match tag filters (skill, keyword), a price range, and a minimum rating, then sort by price, rating, or index time. The module doc is explicit that this is *not* real full-text search, which it calls infeasible to do cheaply in a Soroban contract; instead it maintains a tag/keyword index that clients populate and scans, filters, sorts, and paginates per query. Two integrity rules are enforced structurally: rating is admin-set and never artist-settable, so listings cannot self-inflate their ranking, and deactivation removes a listing from results without losing its history. The contract also accumulates coarse search analytics (`total_searches`, `total_indexed`).

**Dependencies.**

Path deps: `shared` `0.1.0` (`{ path = "../shared", version = "0.1.0" }`) — **declared but never referenced**; `grep` for `shared` across `src/` returns no hits. Registry deps: `soroban-sdk` `21.0.0` (workspace), plus `soroban-sdk` with the `testutils` feature as a dev-dependency. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`. `crate-type = ["cdylib", "rlib"]`. `semver_types.rs` is not a dependency: it is `include!`d from `contracts/semver_types.rs` at `lib.rs:23`.

**Public interface.**

13 externally callable `pub fn`s. The private helpers `has_admin`, `get_admin`, `require_admin`, `load_listing`, `save_listing`, `load_analytics`, `save_analytics`, `contains_string`, `matches_filters`, `is_ordered`, and `sort_listings` are outside the `#[contractimpl]` block and omitted. The three `get_version` / `get_version_metadata` / `is_version_compatible` rows come from the `impl_semver_queries!()` macro invocation at `lib.rs:156`. **`initialize` contains no `require_auth` call** — the `admin.require_auth()` at `lib.rs:48` lives in the private `require_admin` helper, used only by `set_rating`.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `initialize(env: Env, admin: Address) -> Result<(), SearchError>` | **none — no `require_auth` in this function** | `init` | Single-shot via `has_admin`. Because the admin is never authorized, the first caller to reach the contract claims the admin role, making deployment front-runnable; the event payload is a bare `admin` value, not a tuple |
| `get_version` | `get_version(_env: Env) -> ContractVersion` | none (view) | — | Macro-generated |
| `get_version_metadata` | `get_version_metadata(env: Env) -> VersionMetadata` | none (view) | — | Macro-generated; `storage_schema = 1` |
| `is_version_compatible` | `is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated |
| `index_artist` | `index_artist(env: Env, artist: Address, skills: Vec<String>, price: i128, keywords: Vec<String>) -> Result<(), SearchError>` | `artist.require_auth()` | `indexed` | Create-or-update the caller's **own** listing — an artist can only write its own. Requires the contract to be initialized (`NotInitialized` via `has_admin`), rejects `price < 0` (`InvalidPrice`), and enforces `MAX_SKILLS`/`MAX_KEYWORDS`. Rating is preserved from the existing listing (or `0` for a new one) and `active` is reset to `true`, so re-indexing reactivates a deactivated listing. On a first index only, appends to `AllArtists` and increments `total_indexed` |
| `set_rating` | `set_rating(env: Env, artist: Address, rating: u32) -> Result<(), SearchError>` | stored `DataKey::Admin.require_auth()` via `require_admin` | `rating` | The signature takes **no** caller/admin parameter: the address that must sign is read from storage, so an unauthorized caller cannot substitute their own. Rejects `rating > 100` (`InvalidRating`). Deliberately out of the artist's control to prevent self-inflated ranking; `ListingNotFound` for an unindexed artist |
| `deactivate_listing` | `deactivate_listing(env: Env, caller: Address, artist: Address) -> Result<(), SearchError>` | `caller.require_auth()`, plus `caller` must equal `artist` or the stored admin | `deact` | The artist or the admin may toggle it; anyone else gets `Unauthorized`. Sets `active = false`, keeping rating and prior keywords. Payload is the bare `artist` value |
| `reactivate_listing` | `reactivate_listing(env: Env, caller: Address, artist: Address) -> Result<(), SearchError>` | `caller.require_auth()`, plus `caller` must equal `artist` or the stored admin | `react` | Mirror of `deactivate_listing`; sets `active = true` without re-indexing. Payload is the bare `artist` value |
| `get_listing` | `get_listing(env: Env, artist: Address) -> Result<ArtistListing, SearchError>` | none (view) | — | Any caller may read any listing, including inactive ones; no participant check |
| `search` | `search(env: Env, filters: SearchFilters, sort_by: SortBy, page: u32, page_size: u32) -> Result<SearchResultPage, SearchError>` | none (view) | — | The main query. **No auth, yet it is a state mutation**: it increments `total_searches` in `DataKey::Analytics`, so a caller pays for someone else's analytics counter. Guards: not initialized ⇒ `NotInitialized`; `page_size` outside `1..=50` ⇒ `InvalidPageSize`; `min_price > max_price` when both are set ⇒ `InvalidPriceRange`. Filters (`matches_filters`): excludes `active == false`, then exact-match on `skill` and `keyword`, and range checks on `price` and `rating`. Sorts with a stable insertion sort over the whole match set, then slices `[page * page_size, ..)`, returning `total_matches` before pagination. Out-of-range `page` yields an empty result set with the full `total_matches`, not an error. Bounds only the *returned* page — the scan and sort are over every ever-indexed artist |
| `get_analytics` | `get_analytics(env: Env) -> SearchAnalytics` | none (view) | — | `{total_searches, total_indexed}`; `unwrap_or_default()` when unset |
| `get_indexed_count` | `get_indexed_count(env: Env) -> u32` | none (view) | — | `total_indexed` only; not decremented on deactivation, so it counts ever-indexed artists, not currently searchable ones |

**Events.**

Five shapes, all contract-specific — this contract publishes no `shared`-delegated events.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `(symbol_short!("init"),)` | `admin: Address` — a single value, not a tuple | `initialize` |
| `indexed` | `(symbol_short!("indexed"),)` | `(artist: Address, price: i128)` | `index_artist` — published on both create and update; skills, keywords, and rating are not included |
| `rating` | `(symbol_short!("rating"),)` | `(artist: Address, rating: u32)` | `set_rating` |
| `deact` | `(symbol_short!("deact"),)` | `artist: Address` — a single value, not a tuple | `deactivate_listing` |
| `react` | `(symbol_short!("react"),)` | `artist: Address` — a single value, not a tuple | `reactivate_listing` |

**Errors.**

`errors::SearchError`, `#[contracterror]`, `#[repr(u32)]`, unit variants with explicit `= N` discriminants. It derives `PartialOrd`/`Ord` beyond the house pattern, and the file carries a `Display` impl and a `get_suggestion` mapping. Unlike the other three crates there is **no** `src/test.rs` error-code stability pin.

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` found `DataKey::Admin` already present |
| `NotInitialized` | 2 | `index_artist` or `search` found no admin entry — i.e. called before `initialize` |
| `Unauthorized` | 3 | `deactivate_listing` or `reactivate_listing` where `caller` is neither the `artist` nor the stored admin |
| `ListingNotFound` | 4 | No `DataKey::Listing` for the artist — from `set_rating`, `deactivate_listing`, `reactivate_listing`, `get_listing`, and the re-index path inside `index_artist` |
| `InvalidPrice` | 5 | `index_artist` with `price < 0` |
| `InvalidRating` | 6 | `set_rating` with `rating > 100` (there is no lower bound other than the `u32` type) |
| `TooManySkills` | 7 | `index_artist` with `skills.len() > MAX_SKILLS` (20) |
| `TooManyKeywords` | 8 | `index_artist` with `keywords.len() > MAX_KEYWORDS` (30) |
| `InvalidPageSize` | 9 | `search` with `page_size == 0` or `page_size > MAX_PAGE_SIZE` (50) |
| `InvalidPriceRange` | 10 | `search` where both `min_price` and `max_price` are set and `min_price > max_price` |

**Storage.** See [STORAGE.md](./STORAGE.md#search).

**Compile status.** `compiles` — `cargo check -p search --lib` succeeds with no warnings. The 12 unit tests in `src/test.rs` compile against the `rlib` target.


### subscription

> This crate has no `//!` module doc comment. The nearest comment is the `///` doc on the private `charge` fn (contracts/subscription/src/lib.rs:78-80): "Charge a period against the subscriber's prepaid credit. Renewals are driven off this balance rather than a live transfer so that `renew` can be called by anyone once a period ends, without the subscriber signing each time." The first `//` section header in the file is line 151, `// ── Tiers and benefits ──`.

Source: `contracts/subscription/`

**Purpose.**

Sells time-boxed, entitlement-bearing subscription plans ("tiers") to platform members, settling them against a prepaid token balance instead of a live transfer on every billing cycle. An admin defines `Tier` records (name, `price`, `period_ledgers`, a `benefits: Vec<Symbol>` entitlement list, `active`); a member deposits SEP-41 tokens once into a per-account credit balance, and `subscribe` converts the first period's price into credit, so subsequent periods can be charged without the member signing again. `renew` is deliberately permissionless — any keeper or the platform can push the next period once it ends, taking the money from credit the member already authorized, and the billing cycle is contiguous (renewing late closes the gap rather than shifting the window). A configurable `GraceLedgers` window extends entitlement coverage past `period_end_ledger` for auto-renewing subscriptions so a late renewal never causes a service gap, and `lapse` lets anyone mark a lapsed account `Expired`. It exists for the platform operator (admin: tiers, grace window, history cap), for members (deposit, withdraw, subscribe, cancel), and for permissionless keepers (renew, lapse). Beyond the subscription surface it re-exports the shared `health` and `rollout` entry points, so it is also a monitorable, canary-routable contract.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — used, via 8 `shared::health::*` calls and 9 `shared::rollout::*` calls in the delegated health/rollout block. Registry deps: `soroban-sdk.workspace = true` → `21.0.0` from the workspace `[workspace.dependencies]` (resolved to `21.7.7` in `Cargo.lock`); the dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. No other dependencies. `crate-type = ["cdylib", "rlib"]`; `[package.metadata.stellar-aid]` declares `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`.

**Public interface.**

35 externally callable functions: 32 `pub fn`s written in the `#[contractimpl]` block plus 3 generated by the `impl_semver_queries!()` macro (invoked at contracts/subscription/src/lib.rs:149, defined in `contracts/semver_types.rs:86-120`, sourced from this crate's `Cargo.toml` version). Private helpers — `has_admin`, `require_admin`, `require_initialized`, `get_u32`, `load_tier`, `load_subscription`, `save_subscription`, `credit_of`, `set_credit`, `charge`, `record_payment`, `coverage_end`, and the free `is_active` — are not part of the external interface.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, token: Address, grace_ledgers: u32, history_limit: u32) -> Result<(), SubscriptionError>` | none — **permissionless one-shot**; there is no `require_auth()` in the body and `admin` is not checked against anything | `init` | Re-entry guarded: returns `AlreadyInitialized` if `DataKey::Admin` is already set. Rejects `history_limit == 0` with `InvalidAmount`, but applies **no validation to `grace_ledgers`** (any `u32`, including `0`, is accepted). Writes `Admin`, `Token`, `GraceLedgers`, `HistoryLimit`. Because it is unguarded by auth, whoever submits the first transaction owns the contract. Note the `token` parameter shadows the `token` module import inside this fn. |
| `get_version` | `fn get_version(_env: soroban_sdk::Env) -> ContractVersion` | none (view) | — | Macro-generated. Parses `CARGO_PKG_VERSION` (`0.1.0`) into `{major: 0, minor: 1, patch: 0}`; the env arg is unused. |
| `get_version_metadata` | `fn get_version_metadata(env: soroban_sdk::Env) -> VersionMetadata` | none (view) | — | Macro-generated. Returns crate name `"subscription"`, its semver, `min_compatible` (computed as `{0, minor, 0}` for a 0.x crate) and `storage_schema: 1` (the `CURRENT_STORAGE_SCHEMA` const in `semver_types.rs`). Matches the `min-compatible = "0.1.0"` / `storage-schema = 1` metadata in `Cargo.toml`. |
| `is_version_compatible` | `fn is_version_compatible(_env: soroban_sdk::Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated. For a 0.x crate requires an exact `minor` match and `required.patch <= current.patch`. |
| `create_tier` | `fn create_tier(env: Env, tier_id: u32, name: String, price: i128, period_ledgers: u32, benefits: Vec<Symbol>) -> Result<(), SubscriptionError>` | `admin.require_auth()` (stored `DataKey::Admin`, via `require_admin`; missing admin → `NotInitialized`) | `tier_new` | Rejects a duplicate `tier_id` with `TierExists`, `price <= 0` with `InvalidPrice`, and `period_ledgers == 0` with `InvalidPeriod`. Stores the `Tier` with `active: true`. `benefits` is stored verbatim and is what `has_benefit` matches against. |
| `set_tier_active` | `fn set_tier_active(env: Env, tier_id: u32, active: bool) -> Result<(), SubscriptionError>` | `admin.require_auth()` (via `require_admin`) | `tier_set` | Retire/re-open a plan; only flips `tier.active`. Retiring blocks **new** sign-ups (`subscribe` → `TierInactive`) and blocks **renewals** of existing subscribers (`renew` → `TierInactive`), so an existing auto-renew subscriber is cut off at the next period boundary unless the tier is re-activated. Missing tier → `TierNotFound`. |
| `get_tier` | `fn get_tier(env: Env, tier_id: u32) -> Result<Tier, SubscriptionError>` | none (view) | — | Direct passthrough to `load_tier`; unknown id → `TierNotFound`. Does not work before `initialize` in any meaningful way but does not check initialization either — it would just miss. |
| `has_benefit` | `fn has_benefit(env: Env, subscriber: Address, benefit: Symbol) -> bool` | none (view) | — | The single entitlement check. Returns `false` for a missing subscription, for a non-`is_active` subscription, or for a missing tier; otherwise `tier.benefits.contains(&benefit)`. Because it folds in `coverage_end`, an auto-renewing subscription keeps returning `true` through the grace window and a cancelled one stops at `period_end_ledger`. |
| `deposit` | `fn deposit(env: Env, subscriber: Address, amount: i128) -> Result<i128, SubscriptionError>` | `subscriber.require_auth()`, preceded by `require_initialized` (`NotInitialized` if unset) | `deposit` | `amount <= 0` → `InvalidAmount`. Credit is credited **before** the token transfer, and the `token::Client::transfer(&subscriber, &env.current_contract_address(), &amount)` result is discarded (its return value is unused) — a token-contract failure surfaces as a host error that reverts the whole invocation, so the ordering is not exploitable, but the return value is silently dropped. Returns the new balance. |
| `withdraw` | `fn withdraw(env: Env, subscriber: Address, amount: i128) -> Result<i128, SubscriptionError>` | `subscriber.require_auth()`, preceded by `require_initialized` | `withdraw` | `amount <= 0` → `InvalidAmount`; `credit < amount` → `InsufficientCredit`. Debits credit, then transfers out via the token contract. **Withdrawal is not tied to an active subscription or a paid period** — a member can withdraw their entire balance at any time, including credit earmarked for a renewal that has not yet been charged, which will then fail with `InsufficientCredit` at the next `renew`. Returns the remaining balance. |
| `get_credit` | `fn get_credit(env: Env, subscriber: Address) -> i128` | none (view) | — | `credit_of` with a `0` default for unknown accounts. |
| `subscribe` | `fn subscribe(env: Env, subscriber: Address, tier_id: u32, auto_renew: bool) -> Result<(), SubscriptionError>` | `subscriber.require_auth()`, preceded by `require_initialized` | `subbed` | Blocks a second sign-up only if the existing record is `is_active` → `AlreadySubscribed`; a lapsed record is overwritten (history in `Payments` is kept, counters reset). Unknown tier → `TierNotFound`; `!tier.active` → `TierInactive`; insufficient credit → `InsufficientCredit` (from `charge`). No token transfer happens here: the price is taken from credit, so a member must `deposit` first. Writes `Subscription` with `status: Active`, `started_ledger: env.ledger().sequence()`, `period_end_ledger: ledger + tier.period_ledgers`, `renewals: 0`, `total_paid: tier.price`, and appends a `PaymentRecord` with `kind: Initial`, `sequence: 1`. |
| `renew` | `fn renew(env: Env, subscriber: Address) -> Result<u32, SubscriptionError>` | **none — permissionless by design**; the only gate is `require_initialized`. The subscriber does not sign; the funds come from credit they previously authorized | `renewed` | Guards, in order: `status == Expired` → `NoSubscription`; `!auto_renew` → `NotRenewable`; `ledger <= period_end_ledger` → `RenewalNotDue`; `ledger > period_end_ledger + GraceLedgers` → `GraceExpired`; tier missing → `TierNotFound`; `!tier.active` → `TierInactive`; insufficient credit → `InsufficientCredit`. On success advances `period_end_ledger += tier.period_ledgers` (contiguous, never shortened or shifted), `renewals += 1`, `total_paid += tier.price`, charges credit, and appends a `PaymentRecord` with `kind: Renewal`, `sequence: renewals + 1`. Returns the new `period_end_ledger`. **Bumps no TTL.** |
| `cancel` | `fn cancel(env: Env, subscriber: Address) -> Result<u32, SubscriptionError>` | `subscriber.require_auth()`, preceded by `require_initialized` | `cancelled` | Sets `status: Cancelled` and `auto_renew: false`; entitlements then run only to the already-paid `period_end_ledger` (no grace extension, per `coverage_end`). Already-`Expired` → `NoSubscription`. **Irreversible in-contract**: there is no resume/reactivate entry point, and `renew` refuses a non-`auto_renew` subscription with `NotRenewable`; the only way back is a fresh `subscribe` once `is_active` is false. Returns the final `period_end_ledger`. |
| `lapse` | `fn lapse(env: Env, subscriber: Address) -> Result<(), SubscriptionError>` | **none — permissionless**; only `require_initialized` is checked | `lapsed` | Flips `status: Expired` and `auto_renew: false`, but only once coverage has genuinely run out: an `is_active` subscription returns `StillActive`. Because the internal `is_active` returns `true` whenever `status != Expired` and `ledger <= coverage_end`, a cancelled subscription stays non-`Expired` (and therefore un-lapseable) until its paid period elapses. Publishes a **bare `Address` as the payload**, not a tuple. |
| `get_subscription` | `fn get_subscription(env: Env, subscriber: Address) -> Result<Subscription, SubscriptionError>` | none (view) | — | Passthrough to `load_subscription`; unknown subscriber → `NoSubscription`. Returns the stored `status`, which for an auto-renewing subscription can still read `Active` after coverage has lapsed, since only `lapse` writes `Expired`. |
| `is_active` | `fn is_active(env: Env, subscriber: Address) -> bool` | none (view) | — | Returns `false` for a missing subscription; otherwise `status != Expired && env.ledger().sequence() <= coverage_end(...)`. Note the name shadows the private free function `is_active(&Env, &Subscription)` — the body calls the free fn, not itself. |
| `in_grace` | `fn in_grace(env: Env, subscriber: Address) -> bool` | none (view) | — | `true` only when `status != Expired` **and** `auto_renew` **and** `ledger > period_end_ledger` **and** `ledger <= coverage_end`. Strictly narrower than `is_active`; `false` for a missing subscription. |
| `get_payments` | `fn get_payments(env: Env, subscriber: Address) -> Vec<PaymentRecord>` | none (view) | — | Reads `DataKey::Payments(subscriber)` directly, defaulting to an empty `Vec`. Entries beyond `HistoryLimit` are already gone, so the returned length is capped at the admin-set limit. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view, but **writes**: it publishes `hlth_alrt` and stamps `HealthKey::LastAlertLedger`, and can auto-rollback) | delegated — `hlth_alrt`, and `rollback` when the trigger fires | Classifies the contract `Healthy`/`Degraded`/`Unhealthy` from shared metrics; if `report.anomaly` it calls `shared::rollout::maybe_auto_rollback`, which zeroes canary traffic, disables all feature flags, sets phase `RolledBack`, and publishes `rollback`. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | Shared counters (`ok_count`, `error_count`, `last_ok_ledger`, `last_error_ledger`) with `paused` read from `PauseDataKey::Paused`. Only mutated through this contract's `report_ok` / `report_error`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Publishes the SLA constants (availability 9990 bps, max error 10 bps, degraded 100 bps, unhealthy 500 bps, `stall_ledgers` 17280, `health_check_max_ledgers` 60). The `env` arg is explicitly discarded (`let _ = env;`). |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` on the **caller-supplied `admin` parameter** — it is *not* compared with `DataKey::Admin`, so any address the caller names can authorize this | delegated — `alrt_cfg` | Delegates to `shared::health::set_alert_config`, which `panic!("invalid alert config")` (host trap, not a typed contract error) if the bps values exceed 10000, are inverted, or the stall/cooldown ledgers are `0`. Note the auth is a self-selected address, not the contract's stored admin. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to `default_alert_config()` when unset. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | `true` when degraded, unhealthy, or stalled. Unlike `health_check` it emits no event and writes nothing. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter (not the stored admin) | — | Increments the shared `ok_count` and stamps `last_ok_ledger`. Returns nothing. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter | — | Increments the shared `error_count` and stamps `last_error_ledger`. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `feat_flg` | Stores `RolloutKey::Flag(flag)` and appends the flag to `RolloutKey::FlagIndex` if new. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool` | none (view) | — | Returns `false` unconditionally once the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `canary` | Stores canary/stable addresses and the traffic share, derives the phase (`Off` / `Canary` / `Full`). `canary_bps > 10000` panics inside `shared::rollout` (`"canary_bps exceeds 10000"`), i.e. a host trap rather than a typed error. |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky split: `SHA-256(caller XDR) mod 10000 < canary_bps`. `false` when phase is `RolledBack` or `canary_bps == 0`, `true` when `canary_bps == 10000`. The `caller` argument is trusted as given — nothing binds it to the transaction signer. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | Phase, `canary_bps`, optional canary/stable addresses, `rollback_error_bps` (default 500), and the flag count. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `rb_trig` | `error_bps == 0 || > 10000` panics inside `shared::rollout` (`"invalid rollback trigger"`). |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | `true` when the phase is already `RolledBack`, or when the shared error rate meets the trigger with a non-zero sample count. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `rollback`, `contract_paused` | Applies the rollback and additionally sets `PauseDataKey::Paused = true`, publishing a long-form `contract_paused` topic with a `ContractPausedEvent { admin }` payload. There is **no unpause entry point** on this contract. |

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple handed to `publish`. Nine shapes, each from exactly one site; no shape is repeated across entry points.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, token, grace_ledgers)` | `initialize` |
| `tier_new` | `[contract, "tier_new"]` | `(tier_id, price, period_ledgers)` | `create_tier` |
| `tier_set` | `[contract, "tier_set"]` | `(tier_id, active)` | `set_tier_active` |
| `deposit` | `[contract, "deposit"]` | `(subscriber, amount, balance)` | `deposit` |
| `withdraw` | `[contract, "withdraw"]` | `(subscriber, amount)` | `withdraw` |
| `subbed` | `[contract, "subbed"]` | `(subscriber, tier_id, tier.price)` | `subscribe` |
| `renewed` | `[contract, "renewed"]` | `(subscriber, subscription.tier_id, subscription.period_end_ledger)` | `renew` |
| `cancelled` | `[contract, "cancelled"]` | `(subscriber, subscription.period_end_ledger)` | `cancel` |
| `lapsed` | `[contract, "lapsed"]` | `subscriber` — a bare `Address`, **not** a tuple | `lapse` |

The SEP-41 `transfer` performed inside `deposit` and `withdraw` is issued by the **token contract**, not by this one, so it appears in the same transaction under the token's own address. Seven further shapes are reachable through the delegated health/rollout block, published by `contracts/shared/src/{health,rollout}.rs` under this contract's address: `alrt_cfg` `[contract, "alrt_cfg"]` payload `config.unhealthy_error_bps` (via `set_alert_config`); `hlth_alrt` `[contract, "hlth_alrt"]` payload `(status, error_bps, stalled)` (via `health_check`, rate-limited by `alert_cooldown_ledgers`); `canary` `[contract, "canary"]` payload `(canary, stable, canary_bps)`; `feat_flg` `[contract, "feat_flg"]` payload `(flag, enabled)`; `rb_trig` `[contract, "rb_trig"]` payload `error_bps`; `rollback` `[contract, "rollback"]` payload `env.ledger().sequence()` (via `trigger_rollback`, and via `health_check` → `maybe_auto_rollback` when the trigger fires); and `contract_paused` `[contract, "contract_paused"]` payload `ContractPausedEvent { admin }` (via `trigger_rollback`).

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` called when `DataKey::Admin` is already set. Suggestion symbol `DUP`. |
| `NotInitialized` | 2 | Any call to a private `require_admin` / `require_initialized` gate before `DataKey::Admin` exists — i.e. `set_tier_active`, `create_tier`, `deposit`, `withdraw`, `subscribe`, `renew`, `cancel`, `lapse` on a fresh contract. Suggestion `NO_INIT`. |
| `Unauthorized` | 3 | **Declared but never constructed in this crate** — only matched in the `Display` and `get_suggestion` impls. Admin gating goes through `Address::require_auth()`, which raises a host `AuthError`, not this variant. Suggestion `AUTH`. |
| `TierNotFound` | 4 | `load_tier` miss: `set_tier_active`, `get_tier`, `subscribe`, and `renew` (whose stored `tier_id` points at a tier that was never created or was never created for this record). Note `has_benefit` swallows this and returns `false` instead. Suggestion `NO_TIER`. |
| `TierExists` | 5 | `create_tier` with a `tier_id` already present in `DataKey::Tier`. Suggestion `TIER_DUP`. |
| `TierInactive` | 6 | `subscribe` against a tier with `active == false`, or `renew` whose stored tier has been retired. Suggestion `TIER_OFF`. |
| `InvalidPrice` | 7 | `create_tier` with `price <= 0`. Suggestion `BAD_PRICE`. |
| `InvalidPeriod` | 8 | `create_tier` with `period_ledgers == 0`. Suggestion `BAD_PER`. |
| `AlreadySubscribed` | 9 | `subscribe` for an account whose existing `Subscription` is still `is_active` (i.e. within `period_end_ledger`, or within the grace window if auto-renewing). Suggestion `SUB_DUP`. |
| `NoSubscription` | 10 | `load_subscription` miss from `renew`, `cancel`, `get_subscription`; or an already-`Expired` record in `renew` / `cancel` / `lapse`. Suggestion `NO_SUB`. |
| `InsufficientCredit` | 11 | The private `charge` when `credit < price` (from `subscribe` and `renew`), and `withdraw` when `credit < amount`. Suggestion `NO_CRED`. |
| `RenewalNotDue` | 12 | `renew` while `env.ledger().sequence() <= subscription.period_end_ledger` — the current period has not ended. Suggestion `NOT_DUE`. |
| `GraceExpired` | 13 | `renew` when `ledger > period_end_ledger + DataKey::GraceLedgers`, i.e. the grace window has closed. Suggestion `NO_GRACE`. |
| `StillActive` | 14 | `lapse` on a subscription that is still within coverage. Suggestion `ACTIVE`. |
| `NotRenewable` | 15 | `renew` on a subscription with `auto_renew == false` (the state `cancel` puts it in). Suggestion `NO_RENEW`. |
| `InvalidAmount` | 16 | `initialize` with `history_limit == 0`; `deposit` or `withdraw` with `amount <= 0`. Suggestion `BAD_AMT`. |

A few failures do **not** surface as these variants: `deposit` and `withdraw` read `DataKey::Token` with `.unwrap()` (host panic if absent) and discard the SEP-41 `transfer` return value (a token failure reverts as a host error), and the delegated `shared` health/rollout setters `panic!` with string literals (`"invalid alert config"`, `"canary_bps exceeds 10000"`, `"invalid rollback trigger"`).

**Storage.** See [STORAGE.md](./STORAGE.md#subscription).

**Compile status.** `compiles`  > Verified with `cargo check -p subscription -p verification --offline` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`, with no errors and no warnings for this crate. `crate-type = ["cdylib", "rlib"]`, so `SubscriptionContract` and all 35 entry points are reachable. Behaviours that compile cleanly but deserve flagging in the storage docs: `initialize` is unauthenticated (first caller becomes admin); `GraceLedgers` is unvalidated and `get_u32`'s `0` fallback silently disables the grace window; no entry point extends any TTL, so `Tier`, `Subscription`, `Credit`, and `Payments` all depend on the network's default entry lifetimes; and `withdraw` can drain credit earmarked for a future `renew`.


### verification

> This crate has no `//!` module doc comment. The nearest comment is the `///` doc on the quality weights (contracts/verification/src/lib.rs:18-19): "Weights applied to each quality criterion; they sum to 100 so the blended score stays on the same 0..=100 scale as the individual marks." The first `//` section header in the file is line 94, `// ── Verification badges (#598) ──`.

Source: `contracts/verification/`

**Purpose.**

Runs a reviewer-driven quality-verification workflow over member portfolios, so that "verified" status is earned and expiring rather than self-asserted. An artist submits a `Portfolio` (an off-chain `metadata_uri` plus a `work_count`); a reviewer claims it with `start_review` and records a verdict in `review_portfolio` using a four-part `QualityScore`. The weighted blend (`WEIGHT_ORIGINALITY` 30, `WEIGHT_TECHNIQUE` 30, `WEIGHT_CONSISTENCY` 20, `WEIGHT_PRESENTATION` 20) is compared against an admin-set `MinScore`, and approval sets `next_update_ledger = reviewed_ledger + UpdateInterval`, so a verified portfolio goes **stale** after a fixed refresh interval; anyone can then call `flag_update_required` to move it to `UpdateRequired` and stop downstream checks passing. Any artist revision (`update_portfolio`) invalidates the verdict and re-queues the portfolio, with a bounded `VerificationRecord` history. On top of that the contract issues reviewer-attested `Badge` records (`PortfolioVerified`, `IdVerified`, `TopRated`, `ProfessionalCertified`) with per-badge expiry, an issuance/renewal/revocation history, and a per-artist list of badge types ever granted. It serves three audiences: the artist (submit/update, read own status), reviewers and the admin (claim, score, issue and revoke badges; set thresholds), and any downstream contract that needs a single `is_verified` / `is_badge_active` boolean.

**Dependencies.**

Path deps: `shared = { path = "../shared", version = "0.1.0" }` — used, via 8 `shared::health::*` calls and 9 `shared::rollout::*` calls in the delegated health/rollout block. Registry deps: `soroban-sdk.workspace = true` → `21.0.0` from the workspace `[workspace.dependencies]` (resolved to `21.7.7` in `Cargo.lock`); the dev-dependency re-declares `soroban-sdk` with `features = ["testutils"]`. Features: `default = []`, `testutils = ["soroban-sdk/testutils"]`. No other dependencies. `crate-type = ["cdylib", "rlib"]`; `[package.metadata.stellar-aid]` declares `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`.

**Public interface.**

40 externally callable functions: 37 `pub fn`s written in the `#[contractimpl]` block plus 3 generated by the `impl_semver_queries!()` macro (invoked at contracts/verification/src/lib.rs:208, defined in `contracts/semver_types.rs:86-120`, sourced from this crate's `Cargo.toml` version). Private helpers — `has_admin`, `get_admin`, `require_admin`, `require_reviewer`, `get_u32`, `load_portfolio`, `save_portfolio`, `push_history`, `load_badge`, `save_badge`, `push_badge_history`, `track_badge_type`, `badge_is_active`, `overall_score`, and `is_stale` — are not part of the external interface.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `initialize` | `fn initialize(env: Env, admin: Address, min_score: u32, min_work_count: u32, update_interval: u32, history_limit: u32) -> Result<(), VerificationError>` | none — **permissionless one-shot**; no `require_auth()` in the body | `init` | Re-entry guarded: `AlreadyInitialized` if `DataKey::Admin` is set. Rejects `min_score > 100` with `InvalidScore` and `update_interval == 0 \|\| history_limit == 0` with `InvalidInterval`. `min_work_count` is unvalidated. Writes `Admin`, `MinScore`, `MinWorkCount`, `UpdateInterval`, `HistoryLimit`. |
| `get_version` | `fn get_version(_env: soroban_sdk::Env) -> ContractVersion` | none (view) | — | Macro-generated. Parses `CARGO_PKG_VERSION` (`0.1.0`). |
| `get_version_metadata` | `fn get_version_metadata(env: soroban_sdk::Env) -> VersionMetadata` | none (view) | — | Macro-generated. Returns crate name `"verification"`, its semver, `min_compatible` (`{0, minor, 0}` for a 0.x crate) and `storage_schema: 1`, matching the `Cargo.toml` metadata. |
| `is_version_compatible` | `fn is_version_compatible(_env: soroban_sdk::Env, major: u32, minor: u32, patch: u32) -> bool` | none (view) | — | Macro-generated. For 0.x, requires exact `minor` and `required.patch <= current.patch`. |
| `add_reviewer` | `fn add_reviewer(env: Env, reviewer: Address) -> Result<(), VerificationError>` | `admin.require_auth()` (stored `DataKey::Admin`, via `require_admin`; missing admin → `NotInitialized`) | `rev_add` | Stores `DataKey::Reviewer(reviewer) = true`. Idempotent, no duplicate check, and a bare `Address` payload rather than a tuple. Adding the admin's own address is redundant but harmless. |
| `remove_reviewer` | `fn remove_reviewer(env: Env, reviewer: Address) -> Result<(), VerificationError>` | `admin.require_auth()` (via `require_admin`) | `rev_rm` | Removes the allow-list entry. Silently succeeds if the reviewer was never added. Does not revert an in-progress `UnderReview` claim, and the stored `Portfolio.reviewer` field keeps naming them. |
| `is_reviewer` | `fn is_reviewer(env: Env, reviewer: Address) -> bool` | none (view) | — | Pure `DataKey::Reviewer` membership. Returns `false` for the admin address itself unless explicitly added — so `is_reviewer(admin) == false` while `require_reviewer` would still accept them, and `is_badge_active`-style consumers of this getter can disagree with the real gate. |
| `submit_portfolio` | `fn submit_portfolio(env: Env, artist: Address, metadata_uri: String, work_count: u32) -> Result<(), VerificationError>` | `artist.require_auth()`, plus `has_admin` check (`NotInitialized` if unset) | `submitted` | First submission only: an existing `DataKey::Portfolio(artist)` gives `PortfolioExists`. `work_count < MinWorkCount` → `InvalidWorkCount`. Stores `status: Submitted`, `score: 0`, `revision: 1`, `submitted_ledger: env.ledger().sequence()`, `reviewed_ledger: 0`, `reviewer: None`, `next_update_ledger: 0`. **Writes no `History` entry** — the initial submission is not recorded in the audit trail, only revisions and verdicts are. |
| `update_portfolio` | `fn update_portfolio(env: Env, artist: Address, metadata_uri: String, work_count: u32) -> Result<(), VerificationError>` | `artist.require_auth()`, plus `has_admin` check | `updated` | Any revision invalidates the verdict: `status → Submitted`, `score → 0`, `revision += 1`, `submitted_ledger → now`, `reviewed_ledger → 0`, `reviewer → None`, `next_update_ledger → 0` (clearing any refresh deadline). Appends a `Resubmitted` `VerificationRecord` with all-zero quality and an empty `note`, so history notes are only populated by `review_portfolio`. Unknown artist → `PortfolioNotFound`; `work_count` shortfall → `InvalidWorkCount`. |
| `start_review` | `fn start_review(env: Env, reviewer: Address, artist: Address) -> Result<(), VerificationError>` | `require_reviewer` — the reviewer, **or the stored admin**, must be present in `DataKey::Reviewer` and then `reviewer.require_auth()`; unset contract → `NotInitialized`, non-reviewer → `Unauthorized` | `review` | Claim-for-review, so two reviewers do not pick up the same queue entry. Requires `status == Submitted`, else `InvalidStatus`. Sets `status: UnderReview` and `reviewer: Some(reviewer)`. There is no abandon/unclaim entry point, so a reviewer who claims and never scores blocks the portfolio. |
| `review_portfolio` | `fn review_portfolio(env: Env, reviewer: Address, artist: Address, quality: QualityScore, note: String) -> Result<u32, VerificationError>` | `require_reviewer` — reviewer or stored admin, then `reviewer.require_auth()` | `reviewed` | Requires `status == UnderReview`, else `InvalidStatus`; the claimed reviewer is not pinned, so a different reviewer can score someone else's claim. Any component mark `> 100` → `InvalidScore`. The private `overall_score` computes `(originality×30 + technique×30 + consistency×20 + presentation×20) / 100` with integer division, then `approved = score >= MinScore`. On approval: `status: Verified`, `next_update_ledger = ledger + UpdateInterval` (starting the refresh clock). On rejection: `status: Rejected`, `next_update_ledger = 0`. Always sets `score`, `reviewed_ledger`, `reviewer`, and appends a history record carrying the full `QualityScore` and the caller-supplied `note`. Returns the blended score. |
| `flag_update_required` | `fn flag_update_required(env: Env, artist: Address) -> Result<(), VerificationError>` | **none — permissionless**; only `has_admin` (initialized) is checked | `stale` | The intended freshness enforcer. Requires the private `is_stale` — `status == Verified && next_update_ledger > 0 && env.ledger().sequence() > next_update_ledger` — else `UpdateNotDue`. Sets `status: UpdateRequired`. Because `is_stale` demands `status == Verified`, a portfolio already moved to `UpdateRequired` (or to `Rejected`) can never be flagged again, and the transition is one-way: nothing in the contract sets the status back to `Verified` except a new `review_portfolio` cycle via `update_portfolio`. Publishes a bare `Address` payload. |
| `get_portfolio` | `fn get_portfolio(env: Env, artist: Address) -> Result<Portfolio, VerificationError>` | none (view) | — | Passthrough to `load_portfolio`; unknown artist → `PortfolioNotFound`. Returns the **stored** status, which can disagree with derived freshness: a `Verified` portfolio past `next_update_ledger` still reads `Verified` here until someone calls `flag_update_required`. |
| `get_history` | `fn get_history(env: Env, artist: Address) -> Vec<VerificationRecord>` | none (view) | — | Reads `DataKey::History(artist)`, defaulting to an empty `Vec`. Capped at `HistoryLimit` by `push_history`. |
| `is_verified` | `fn is_verified(env: Env, artist: Address) -> bool` | none (view) | — | The primary downstream gate: `status == Verified && !is_stale(...)`, `false` for an unknown artist. Derives staleness, so unlike `get_portfolio` it goes false the moment the refresh deadline passes, with no state write needed. |
| `requires_update` | `fn requires_update(env: Env, artist: Address) -> bool` | none (view) | — | `true` when `status == UpdateRequired` **or** `is_stale(...)`, `false` for an unknown artist. |
| `issue_badge` | `fn issue_badge(env: Env, reviewer: Address, artist: Address, badge_type: BadgeType, valid_for_ledgers: u32, note: String) -> Result<(), VerificationError>` | `require_reviewer` — reviewer or stored admin, then `reviewer.require_auth()` | `badge` | Issue **or renew**. A prior badge that is *not* revoked is treated as a renewal (`BadgeAction::Renewed`) and its expiry is recomputed as `ledger + valid_for_ledgers`; a previously revoked or absent badge starts fresh (`BadgeAction::Issued`). `valid_for_ledgers == 0` means the badge never expires on its own. Note it does **not** consult the portfolio workflow despite `BadgeType::PortfolioVerified` existing — a reviewer can issue `PortfolioVerified` to an artist with no portfolio at all, and the `note` is recorded only in `BadgeHistory` (the `Badge` struct has no note field). Overwrites the whole `Badge` record, clearing `revoke_reason` and resetting `issuer`/`issued_ledger`. |
| `revoke_badge` | `fn revoke_badge(env: Env, reviewer: Address, artist: Address, badge_type: BadgeType, reason: String) -> Result<(), VerificationError>` | `require_reviewer` — reviewer or stored admin, then `reviewer.require_auth()` | `bdg_rvk` | Sets `status: Revoked` and stores `reason` in `revoke_reason`, then appends a `BadgeAction::Revoked` history event. Missing badge → `BadgeNotFound`; already revoked → `BadgeAlreadyRevoked`. Revocation is permanent until a fresh `issue_badge`, which clears `revoke_reason`. Works on a badge that has not yet expired and on a never-expiring one. |
| `get_badge` | `fn get_badge(env: Env, artist: Address, badge_type: BadgeType) -> Result<Badge, VerificationError>` | none (view) | — | Returns the stored `Badge` regardless of status or expiry; `BadgeNotFound` when the (artist, type) pair was never issued. A caller must run `is_badge_active` separately to honour the expiry. |
| `is_badge_active` | `fn is_badge_active(env: Env, artist: Address, badge_type: BadgeType) -> bool` | none (view) | — | `!status.revoked() && (expires_ledger == 0 || env.ledger().sequence() <= expires_ledger)`; `false` for a never-issued badge. **Inclusive** of `expires_ledger` itself. |
| `get_artist_badge_types` | `fn get_artist_badge_types(env: Env, artist: Address) -> Vec<BadgeType>` | none (view) | — | Reads `DataKey::BadgeTypes(artist)`, defaulting to an empty `Vec`. Lists every type ever issued, including revoked ones — the doc comment directs callers to `is_badge_active`/`get_badge` for current status. |
| `get_badge_history` | `fn get_badge_history(env: Env, artist: Address) -> Vec<BadgeEvent>` | none (view) | — | Reads `DataKey::BadgeHistory(artist)`, defaulting to an empty `Vec`. Capped at `HistoryLimit`. |
| `set_min_score` | `fn set_min_score(env: Env, min_score: u32) -> Result<(), VerificationError>` | `admin.require_auth()` (via `require_admin`) | — | `min_score > 100` → `InvalidScore`. Does not re-score existing portfolios and does not change any stored verdict. |
| `set_update_interval` | `fn set_update_interval(env: Env, update_interval: u32) -> Result<(), VerificationError>` | `admin.require_auth()` (via `require_admin`) | — | `update_interval == 0` → `InvalidInterval`. Applies only to future approvals: existing `next_update_ledger` values are left untouched, so shortening the interval does not accelerate current refresh deadlines. There is no setter for `MinWorkCount` or `HistoryLimit`. |
| `health_check` | `fn health_check(env: Env) -> shared::health::HealthReport` | none (view, but **writes**: it publishes `hlth_alrt`, stamps `HealthKey::LastAlertLedger`, and can auto-rollback) | delegated — `hlth_alrt`, and `rollback` when the trigger fires | Classifies `Healthy`/`Degraded`/`Unhealthy` from shared metrics; calls `shared::rollout::maybe_auto_rollback` when `report.anomaly`, which zeroes canary traffic, disables all flags, sets phase `RolledBack`, and publishes `rollback`. |
| `get_health_metrics` | `fn get_health_metrics(env: Env) -> shared::health::HealthMetrics` | none (view) | — | Shared counters with `paused` read from `PauseDataKey::Paused`; only mutated through this contract's `report_ok`/`report_error`. |
| `get_sla_targets` | `fn get_sla_targets(env: Env) -> shared::health::SlaTargets` | none (view) | — | Publishes the SLA constants (availability 9990 bps, max error 10 bps, degraded 100 bps, unhealthy 500 bps, `stall_ledgers` 17280, `health_check_max_ledgers` 60). The `env` arg is explicitly discarded. |
| `set_alert_config` | `fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig)` | `admin.require_auth()` on the **caller-supplied `admin` parameter** — not compared with `DataKey::Admin` | delegated — `alrt_cfg` | Delegates to `shared::health::set_alert_config`, which `panic!("invalid alert config")` (host trap) if bps values exceed 10000, are inverted, or the stall/cooldown ledgers are `0`. |
| `get_alert_config` | `fn get_alert_config(env: Env) -> shared::health::AlertConfig` | none (view) | — | Falls back to `default_alert_config()` when unset. |
| `detect_anomaly` | `fn detect_anomaly(env: Env) -> bool` | none (view) | — | `true` when degraded, unhealthy, or stalled. Emits no event and writes nothing. |
| `report_ok` | `fn report_ok(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter | — | Increments shared `ok_count`, stamps `last_ok_ledger`. |
| `report_error` | `fn report_error(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter | — | Increments shared `error_count`, stamps `last_error_ledger`. |
| `set_feature_flag` | `fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `feat_flg` | Stores `RolloutKey::Flag(flag)`, appending to `RolloutKey::FlagIndex` if new. |
| `is_feature_enabled` | `fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool` | none (view) | — | `false` unconditionally once the rollout phase is `RolledBack`. |
| `set_canary_deployment` | `fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `canary` | Stores canary/stable plus the traffic share and derives the phase; `canary_bps > 10000` panics inside `shared::rollout` (`"canary_bps exceeds 10000"`). |
| `route_to_canary` | `fn route_to_canary(env: Env, caller: Address) -> bool` | none (view) | — | Sticky split on `SHA-256(caller XDR) mod 10000 < canary_bps`; `false` when phase is `RolledBack` or the share is `0`, `true` at `10000`. The `caller` argument is trusted as given — nothing binds it to the signer. |
| `get_rollout_state` | `fn get_rollout_state(env: Env) -> shared::rollout::RolloutState` | none (view) | — | Phase, `canary_bps`, optional canary/stable addresses, `rollback_error_bps` (default 500), flag count. |
| `set_rollback_trigger` | `fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `rb_trig` | `error_bps == 0 \|\| > 10000` panics inside `shared::rollout` (`"invalid rollback trigger"`). |
| `should_rollback` | `fn should_rollback(env: Env) -> bool` | none (view) | — | `true` when the phase is already `RolledBack`, or when the shared error rate meets the trigger with a non-zero sample count. |
| `trigger_rollback` | `fn trigger_rollback(env: Env, admin: Address)` | `admin.require_auth()` on the caller-supplied parameter | delegated — `rollback`, `contract_paused` | Applies the rollback and sets `PauseDataKey::Paused = true`, publishing a long-form `contract_paused` topic with a `ContractPausedEvent { admin }` payload. There is **no unpause entry point** here. |

**Events.**

Topics are listed as `[contract address, …]`, since the contract address is always prepended to the topic tuple handed to `publish`. Ten shapes from ten `publish` call sites, one per event; the two `reviewed` topic variants come from a single conditional site and are listed as one row.

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `init` | `[contract, "init"]` | `(admin, min_score, min_work_count, update_interval)` | `initialize` |
| `rev_add` | `[contract, "rev_add"]` | `reviewer` — a bare `Address`, **not** a tuple | `add_reviewer` |
| `rev_rm` | `[contract, "rev_rm"]` | `reviewer` — a bare `Address`, **not** a tuple | `remove_reviewer` |
| `submitted` | `[contract, "submitted"]` | `(artist, work_count)` | `submit_portfolio` |
| `updated` | `[contract, "updated"]` | `(artist, revision)` | `update_portfolio` |
| `review` | `[contract, "review"]` | `(artist, reviewer)` | `start_review` |
| `reviewed` | `[contract, "reviewed", "approved"]` or `[contract, "reviewed", "rejected"]` | `(artist, reviewer, score)` | `review_portfolio` — one site; the second topic is the conditional `approved` / `rejected` symbol |
| `stale` | `[contract, "stale"]` | `artist` — a bare `Address`, **not** a tuple | `flag_update_required` |
| `badge` | `[contract, "badge"]` | `(artist, badge_type, reviewer, expires_ledger)` | `issue_badge` — `expires_ledger` is `0` for a never-expiring badge; issuance and renewal are indistinguishable in the event (the `Issued`/`Renewed` distinction lives only in `BadgeHistory`) |
| `bdg_rvk` | `[contract, "bdg_rvk"]` | `(artist, badge_type, reviewer)` | `revoke_badge` — the `reason` argument is **not** in the payload; it is stored in `Badge.revoke_reason` and in `BadgeHistory` |

Note that `set_min_score` and `set_update_interval` emit nothing. Seven further shapes are reachable through the delegated health/rollout block, published by `contracts/shared/src/{health,rollout}.rs` under this contract's address: `alrt_cfg` `[contract, "alrt_cfg"]` payload `config.unhealthy_error_bps`; `hlth_alrt` `[contract, "hlth_alrt"]` payload `(status, error_bps, stalled)` (rate-limited by `alert_cooldown_ledgers`); `canary` `[contract, "canary"]` payload `(canary, stable, canary_bps)`; `feat_flg` `[contract, "feat_flg"]` payload `(flag, enabled)`; `rb_trig` `[contract, "rb_trig"]` payload `error_bps`; `rollback` `[contract, "rollback"]` payload `env.ledger().sequence()`; and `contract_paused` `[contract, "contract_paused"]` payload `ContractPausedEvent { admin }`.

**Errors.**

| Variant | Code | Condition |
|---|---|---|
| `AlreadyInitialized` | 1 | `initialize` called when `DataKey::Admin` is already set. Suggestion symbol `DUP`. |
| `NotInitialized` | 2 | Any gated entry point reached before `DataKey::Admin` exists: via `require_admin` for `add_reviewer`, `remove_reviewer`, `set_min_score`, `set_update_interval`; via `require_reviewer` for `start_review`, `review_portfolio`, `issue_badge`, `revoke_badge`; and via the explicit `has_admin` checks in `submit_portfolio`, `update_portfolio`, `flag_update_required`. Note the bare getters (`get_portfolio`, `get_history`, `is_verified`, `requires_update`, `get_badge`, `is_badge_active`, `get_artist_badge_types`, `get_badge_history`, `is_reviewer`) do **not** check initialization and instead return a miss/empty/`false`. Suggestion `NO_INIT`. |
| `Unauthorized` | 3 | `require_reviewer` when the signer is neither the stored `DataKey::Admin` nor present in `DataKey::Reviewer(reviewer)`. Affects `start_review`, `review_portfolio`, `issue_badge`, `revoke_badge`. Suggestion `AUTH`. |
| `PortfolioNotFound` | 4 | `load_portfolio` miss from `update_portfolio`, `start_review`, `review_portfolio`, `flag_update_required`, and from `get_portfolio`. Swallowed (returns `false`) by `is_verified` and `requires_update`. Suggestion `NOT_FOUND`. |
| `PortfolioExists` | 5 | `submit_portfolio` for an artist who already has a `DataKey::Portfolio` entry, whatever its status. Suggestion `EXISTS`. |
| `InvalidStatus` | 6 | `start_review` when `status != Submitted`; `review_portfolio` when `status != UnderReview`. Suggestion `BAD_STS`. |
| `InvalidScore` | 7 | `initialize` or `set_min_score` with `min_score > 100`; `review_portfolio` when any `QualityScore` component (`originality`, `technique`, `consistency`, `presentation`) exceeds `100`. Suggestion `BAD_SCORE`. |
| `InvalidWorkCount` | 8 | `submit_portfolio` or `update_portfolio` with `work_count < DataKey::MinWorkCount`. Suggestion `BAD_WORK`. |
| `InvalidInterval` | 9 | `initialize` with `update_interval == 0` or `history_limit == 0`; `set_update_interval` with `0`. Suggestion `BAD_INTVL`. |
| `UpdateNotDue` | 10 | `flag_update_required` on a portfolio that is not `(status == Verified && next_update_ledger > 0 && ledger > next_update_ledger)` — i.e. not yet stale, already flagged, or never approved. Suggestion `NOT_DUE`. |
| `BadgeNotFound` | 11 | `load_badge` miss in `revoke_badge`, and in `get_badge`. `issue_badge` treats a miss as a fresh issuance, and `is_badge_active` turns it into `false`. Suggestion `NO_BADGE`. |
| `BadgeAlreadyRevoked` | 12 | `revoke_badge` on a badge whose `status` is already `Revoked`. Suggestion `REVOKED`. |

Failures that do **not** surface as these variants: the delegated `shared` setters `panic!` with string literals (`"invalid alert config"`, `"canary_bps exceeds 10000"`, `"invalid rollback trigger"`), and no `panic!`/`assert!`/string-literal abort exists anywhere in this crate's own `src/`.

**Storage.** See [STORAGE.md](./STORAGE.md#verification).

**Compile status.** `compiles`  > Verified with `cargo check -p subscription -p verification --offline` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`, with no errors and no warnings for this crate. `crate-type = ["cdylib", "rlib"]`, so `Verification` and all 40 entry points are reachable. Behaviours that compile cleanly but deserve flagging in the storage docs: `initialize` is unauthenticated (first caller becomes admin); no entry point extends any TTL, so `Portfolio`, `History`, `Badge`, `BadgeHistory`, and `BadgeTypes` all live on the network's default entry lifetimes while the refresh and expiry windows are pure ledger comparisons; `submit_portfolio` writes no `History` record; `issue_badge` can grant `PortfolioVerified` with no portfolio at all; and `is_reviewer` disagrees with the real gate in `require_reviewer` for the admin address.


### shared

> State migration and upgrade safety helpers (closes #595). — module doc of `upgrade.rs`; the crate is a 10-module library (`config`, `errors`, `pause`, `types`, `upgrade`, `health`, `rollout`, `correlation`, `validation`, `version`) with no crate-level `//!` doc.

Source: `contracts/shared/`

**Purpose.**

`shared` is a `#![no_std]` plain library (`crate-type = ["lib"]`, `publish = false`) that supplies the cross-cutting machinery every other Lumora contract composes instead of reimplementing: an emergency pause with a time-locked recovery, a unified error-code catalogue, semver/storage-schema negotiation, on-chain health metrics with alerting, gradual (canary) rollout with sticky traffic splitting and rollback, inter-contract event correlation, typed `config_contract` lookups, and input-length limits. It declares no `#[contract]` type and no `#[contractimpl]` block, so it contributes no contract entry points of its own — its functions are generic over `&Env` and are inlined into the consuming contract's WASM, meaning they act on **the caller's** storage namespaces. 24 of the 25 `contracts/*/Cargo.toml` files declare `shared` as a path dependency; `analytics` is the notable exception. Its public helper set is deliberately kept stable: `pause::pause(&env, &admin)` / `pause::unpause(&env, &admin)` signatures are documented as *frozen* because escrow, campaign, donation, withdrawal and revenue_sharing call them as bare statements.

**Dependencies.**

No path deps. `soroban-sdk` `21.0.0` (inherited from `[workspace.dependencies]`). `[package.metadata.stellar-aid]`: `versioning = "semver"`, `storage-schema = 1`, `min-compatible = "0.1.0"`.

**Public interface.**

There is no `#[contractimpl]` block, so no row below is an addressable contract entry point; these are the `pub fn`s that consuming contracts call from their own `#[contractimpl]`s (e.g. `audit` re-exports 19 of them verbatim). Internal `fn`s (`health::save_metrics`, `health::is_paused`, `health::last_activity_ledger`, `health::maybe_emit_alert`, `rollout::flag_index`, `rollout::save_flag_index`, `rollout::phase_from_bps`, `rollout::disable_all_flags`, `rollout::apply_rollback`, `config::try_call`, `config::emit_config_lookup_failed`) are omitted.

| Function | Signature | Authorization | Emits | Notes |
|---|---|---|---|---|
| `is_paused` | `pause::is_paused(env: &Env) -> bool` | none (view) | — | Public read of the pause flag; `health.rs` has a private duplicate |
| `recovery_eta` | `pause::recovery_eta(env: &Env) -> Option<u32>` | none (view) | — | Returns `None` when the stored value is `0` |
| `require_not_paused_typed` | `pause::require_not_paused_typed(env: &Env) -> Result<(), PauseError>` | none (view) | — | Typed guard; the 5 consumer contracts have **not** migrated to it (documented as deliberate in #711) |
| `require_not_paused` | `pause::require_not_paused(env: &Env)` | none (view) | — | Aborts with the `PauseError` `Display` string; kept for the existing bare-statement call sites |
| `pause` | `pause::pause(env: &Env, admin: &Address)` | `admin.require_auth()` | `contract_paused` | Also clears `RecoveryEta` to `0` — a fresh stop invalidates any armed lock |
| `schedule_recovery` | `pause::schedule_recovery(env: &Env, admin: &Address, delay_ledgers: u32) -> Result<u32, PauseError>` | `admin.require_auth()` | `recovery_scheduled` | Requires the contract to be paused; bounds `delay` to `1..=MAX_RECOVERY_DELAY_LEDGERS`; returns the maturing ledger via `checked_add` |
| `cancel_recovery` | `pause::cancel_recovery(env: &Env, admin: &Address) -> Result<(), PauseError>` | `admin.require_auth()` | — | Disarms the lock; contract stays paused |
| `try_unpause` | `pause::try_unpause(env: &Env, admin: &Address) -> Result<(), PauseError>` | `admin.require_auth()` | `recovery_completed` (only when a lock matured), `contract_unpaused` | Refuses with `RecoveryPending` while `ledger() < eta`; clears the lock then sets `Paused = false` |
| `unpause` | `pause::unpause(env: &Env, admin: &Address)` | `admin.require_auth()` (via `try_unpause`) | `recovery_completed`, `contract_unpaused` | Signature frozen — cannot return `Result` without breaking the 5 consumers and failing `clippy -D warnings`; panics on `PauseError` |
| `get_suggestion` | `pause::get_suggestion(error: PauseError) -> Symbol` | none (view) | — | Maps to `PAUSED`/`LOCKED`/`NO_RECOV`/`BAD_DELAY`/`UNPAUSED` |
| `record_upgrade` | `upgrade::record_upgrade(env: &Env, admin: &Address, version: ContractVersion)` | `admin.require_auth()` | `upgraded` | Writes `Version` + `LastUpgradeLedger`; payload is `(prev, version, ledger)` |
| `get_version` | `upgrade::get_version(env: &Env) -> ContractVersion` | none (view) | — | Falls back to `{0,0,0}` when never seeded |
| `require_paused_for_upgrade` | `upgrade::require_paused_for_upgrade(env: &Env)` | none (view) | — | Panics `"contract must be paused before upgrading"`; reads `PauseDataKey::Paused` |
| `signal_migration_needed` | `upgrade::signal_migration_needed(env: &Env, from_version: ContractVersion, to_version: ContractVersion)` | none (view) | `mig_need` | No auth check inside the library — the caller must gate it |
| `default_alert_config` | `health::default_alert_config() -> AlertConfig` | none (view) | — | Pure constructor from the `DEFAULT_*` constants |
| `sla_targets` | `health::sla_targets() -> SlaTargets` | none (view) | — | Published SLA numbers for monitors |
| `get_alert_config` | `health::get_alert_config(env: &Env) -> AlertConfig` | none (view) | — | Falls back to `default_alert_config()` |
| `set_alert_config` | `health::set_alert_config(env: &Env, config: AlertConfig)` | **none inside the library** — `unknown — no `require_auth()` in this function; every consumer that wraps it (e.g. `audit::set_alert_config`) adds its own `admin.require_auth()`** | `alrt_cfg` | Panics on bps > 10000, degraded > unhealthy, or zero stall/cooldown |
| `get_metrics` | `health::get_metrics(env: &Env) -> HealthMetrics` | none (view) | — | Overlays the live pause flag onto the stored record |
| `record_ok` | `health::record_ok(env: &Env)` | none inside the library | — | `saturating_add(1)` on `ok_count`, stamps `last_ok_ledger` |
| `record_error` | `health::record_error(env: &Env)` | none inside the library | — | Same for `error_count` / `last_error_ledger` |
| `error_bps` | `health::error_bps(metrics: &HealthMetrics) -> u32` | none (view) | — | `errors * 10_000 / (ok + errors)`, `0` when no samples; carries `#[allow(clippy::manual_checked_ops)]` |
| `is_stalled` | `health::is_stalled(env: &Env, metrics: &HealthMetrics, config: &AlertConfig) -> bool` | none (view) | — | `false` when last activity is `0` |
| `classify` | `health::classify(env: &Env, metrics: &HealthMetrics, config: &AlertConfig) -> (HealthStatus, bool)` | none (view) | — | Paused ⇒ `Unhealthy`; rate ≥ unhealthy bps ⇒ `Unhealthy`; rate ≥ degraded bps or stalled ⇒ `Degraded` |
| `detect_anomaly` | `health::detect_anomaly(env: &Env) -> bool` | none (view) | — | Read-only wrapper over `classify(..).1` |
| `health_check` | `health::health_check(env: &Env) -> HealthReport` | none (view) | `hlth_alrt` (conditional) | Classifies, then may emit an alert subject to the cooldown |
| `get_canary_bps` | `rollout::get_canary_bps(env: &Env) -> u32` | none (view) | — | Defaults to `0` |
| `get_phase` | `rollout::get_phase(env: &Env) -> RolloutPhase` | none (view) | — | Defaults to `Off` |
| `get_rollback_error_bps` | `rollout::get_rollback_error_bps(env: &Env) -> u32` | none (view) | — | Defaults to `DEFAULT_ROLLBACK_ERROR_BPS` |
| `get_state` | `rollout::get_state(env: &Env) -> RolloutState` | none (view) | — | Aggregates the six rollout keys plus `flag_count` |
| `set_canary_deployment` | `rollout::set_canary_deployment(env: &Env, canary: Address, stable: Address, canary_bps: u32)` | **none inside the library** | `canary` | Panics above 10000 bps; derives `Phase` via `phase_from_bps` |
| `set_canary_bps` | `rollout::set_canary_bps(env: &Env, canary_bps: u32)` | **none inside the library** | — | Updates bps and re-derives `Phase`; does **not** update `Canary`/`Stable` |
| `route_to_canary` | `rollout::route_to_canary(env: &Env, caller: &Address) -> bool` | none (view) | — | Sticky split: `SHA-256(caller.to_xdr())` first 4 bytes as `u32` `% 10_000 < canary_bps`; `false` when `RolledBack` or bps `0` |
| `set_feature_flag` | `rollout::set_feature_flag(env: &Env, flag: &Symbol, enabled: bool)` | **none inside the library** | `feat_flg` | Maintains `FlagIndex` (append-if-absent) |
| `is_feature_enabled` | `rollout::is_feature_enabled(env: &Env, flag: &Symbol) -> bool` | none (view) | — | Forced `false` while `RolledBack` |
| `set_rollback_trigger` | `rollout::set_rollback_trigger(env: &Env, error_bps: u32)` | **none inside the library** | `rb_trig` | Panics when `0` or above 10000 |
| `should_rollback` | `rollout::should_rollback(env: &Env) -> bool` | none (view) | — | `true` if already `RolledBack`, or error bps ≥ trigger with at least one sample |
| `maybe_auto_rollback` | `rollout::maybe_auto_rollback(env: &Env) -> bool` | none inside the library | `rollback` | No-op unless canary bps > 0 or flags exist; then `apply_rollback` |
| `trigger_rollback` | `rollout::trigger_rollback(env: &Env, admin: &Address)` | **none inside the library** — doc says "Caller must already have authorized `admin`"; the wrapper is expected to `require_auth()` once to avoid a double-auth `HostError` | `rollback`, `contract_paused` | Sets `CanaryBps = 0`, `Phase = RolledBack`, disables all flags, and sets `PauseDataKey::Paused = true`; deliberately neither arms nor clears a recovery lock |
| `CorrelationId::derive` | `correlation::CorrelationId::derive(env: &Env, scope: &Bytes, parts: &[&Bytes]) -> CorrelationId` | none (view) | — | SHA-256 over `scope` plus each part length-prefixed as 4 big-endian bytes |
| `CorrelationId::link` | `correlation::CorrelationId::link(env: &Env, parent: &CorrelationId, child: &CorrelationId) -> CorrelationId` | none (view) | — | `sha256(parent ∥ child)` — encodes causal order for indexers |
| `CorrelationId::to_bytes` | `correlation::CorrelationId::to_bytes(&self, env: &Env) -> Bytes` | none (view) | — | Opaque 32-byte slice for payloads |
| `scope` | `correlation::scope(env: &Env, domain: &str) -> Bytes` | none (view) | — | Canonical domain bytes, e.g. `"escrow"`, `"agr"`, `"dispute"` |
| `publish` | `correlation::publish<S: Into<Symbol>>(env: &Env, domain: S, action: S, id: &CorrelationId, key: &Bytes)` | none (view) | `(domain, action, "corr")` | Reserves the third topic element so `docs/EVENTS.md` primary schemas stay unchanged |
| `try_get_fee_bps` | `config::try_get_fee_bps(env: &Env, config_contract: &Address) -> Result<u32, ()>` | none (view) | `cfg_fail` on failure | Invokes selector `get_fee_b` |
| `try_get_usdc` | `config::try_get_usdc(env: &Env, config_contract: &Address) -> Result<Address, ()>` | none (view) | `cfg_fail` on failure | Selector `get_usdc` |
| `try_get_admin` | `config::try_get_admin(env: &Env, config_contract: &Address) -> Result<Address, ()>` | none (view) | `cfg_fail` on failure | Selector `get_adm` |
| `try_get_platform_wallet` | `config::try_get_platform_wallet(env: &Env, config_contract: &Address) -> Result<Address, ()>` | none (view) | `cfg_fail` on failure | Selector `get_pw` |
| `require_string_len` | `validation::require_string_len(s: &String, max_len: u32, field: &str)` | none (view) | — | Panics past the limit; `String::len()` is a byte count |
| `is_string_len_valid` | `validation::is_string_len_valid(s: &String, max_len: u32) -> bool` | none (view) | — | Non-panicking form |
| `require_id_len` | `validation::require_id_len(id: &Bytes, max_len: u32, field: &str)` | none (view) | — | Panics past the limit |
| `is_id_len_valid` | `validation::is_id_len_valid(id: &Bytes, max_len: u32) -> bool` | none (view) | — | Non-panicking form |
| `parse_semver` | `version::parse_semver(raw: &str) -> ContractVersion` | none (view) | — | Stops at the first non-digit/non-`.`; tolerates partials and `-prerelease` |
| `min_compatible_for` | `version::min_compatible_for(current: &ContractVersion) -> ContractVersion` | none (view) | — | `0.x` ⇒ `{0, minor, 0}`; `>=1.0` ⇒ `{major, 0, 0}` |
| `store` | `version::store(env: &Env, version: &ContractVersion)` | **none inside the library** | — | Writes `UpgradeKey::Version`, keeping `upgrade::get_version` and `version::query` in sync |
| `seed` | `version::seed(env: &Env, pkg_version: &str)` | **none inside the library** | — | `store` + `parse_semver`; meant to be called from a consumer's `initialize` |
| `query` | `version::query(env: &Env, pkg_version: &str) -> ContractVersion` | none (view) | — | Stored version, else the WASM crate version |
| `query_metadata` | `version::query_metadata(env: &Env, pkg_name: &str, pkg_version: &str) -> VersionMetadata` | none (view) | — | Adds `min_compatible` and `CURRENT_STORAGE_SCHEMA` |
| `ContractVersion::new` | `version::ContractVersion::new(major: u32, minor: u32, patch: u32) -> Self` | none (view) | — | `impl` block on the `#[contracttype]` struct |
| `ContractVersion::cmp_semver` | `version::ContractVersion::cmp_semver(&self, other: &Self) -> core::cmp::Ordering` | none (view) | — | major, then minor, then patch |
| `ContractVersion::is_compatible_with` | `version::ContractVersion::is_compatible_with(&self, required: &Self) -> bool` | none (view) | — | Major must match; on `0.x` minor must also match and only patch may drift; on `>=1.0` any newer minor/patch of the same major is accepted |
| `ContractVersion::satisfies` | `version::ContractVersion::satisfies(&self, constraint: &VersionConstraint) -> bool` | none (view) | — | `self.major <= constraint.max_major && is_compatible_with(&constraint.min)` |
| `impl_semver_queries!` | `macro_rules! impl_semver_queries` (`#[macro_export]`) | — | — | `macro_rules!`, not a `fn`: installs `get_version`, `get_version_metadata`, `is_version_compatible` on the caller's `#[contractimpl]`, reading `env!("CARGO_PKG_NAME")` / `env!("CARGO_PKG_VERSION")` and delegating to `version::query` (so it *does* consult `UpgradeKey::Version`). Contracts that do not depend on `shared` get a Cargo.toml-only twin from `contracts/semver_types.rs` via `include!` |

**Events.**

| Event | Topics | Payload | Emitted by |
|---|---|---|---|
| `contract_paused` | `(Symbol::new(env, "contract_paused"),)` | `ContractPausedEvent { admin }` | `pause::pause`; also `rollout::trigger_rollback` (same `Symbol::new` name and payload type) |
| `contract_unpaused` | `(Symbol::new(env, "contract_unpaused"),)` | `ContractUnpausedEvent { admin }` | `pause::try_unpause` / `pause::unpause` |
| `recovery_scheduled` | `(Symbol::new(env, "recovery_scheduled"),)` | `RecoveryScheduledEvent { admin, eta_ledger: u32, delay_ledgers: u32 }` | `pause::schedule_recovery` |
| `recovery_completed` | `(Symbol::new(env, "recovery_completed"),)` | `RecoveryCompletedEvent { admin, eta_ledger: u32 }` | `pause::try_unpause` / `pause::unpause`, only when a lock actually matured |
| `upgraded` | `(symbol_short!("upgraded"),)` | `(prev: Option<ContractVersion>, version: ContractVersion, ledger: u32)` | `upgrade::record_upgrade` |
| `mig_need` | `(symbol_short!("mig_need"),)` | `(from_version, to_version)` | `upgrade::signal_migration_needed` |
| `alrt_cfg` | `(symbol_short!("alrt_cfg"),)` | `config.unhealthy_error_bps` (bare `u32`, not a tuple) | `health::set_alert_config` |
| `hlth_alrt` | `(symbol_short!("hlth_alrt"),)` | `(status: HealthStatus, error_bps: u32, stalled: bool)` | `health::health_check` via the private `maybe_emit_alert`; rate-limited by `AlertConfig::alert_cooldown_ledgers` and suppressed when `alerting_enabled == false` |
| `canary` | `(symbol_short!("canary"),)` | `(canary: Address, stable: Address, canary_bps: u32)` | `rollout::set_canary_deployment` |
| `feat_flg` | `(symbol_short!("feat_flg"),)` | `(flag: Symbol, enabled: bool)` | `rollout::set_feature_flag` |
| `rb_trig` | `(symbol_short!("rb_trig"),)` | `error_bps` (bare `u32`) | `rollout::set_rollback_trigger` |
| `rollback` | `(symbol_short!("rollback"),)` | `env.ledger().sequence()` (bare `u32`) | `rollout::apply_rollback`, reached from `maybe_auto_rollback` and `trigger_rollback` |
| `(domain, action, "corr")` | `(domain: Symbol, action: Symbol, Symbol::new(env, "corr"))` | `(id: BytesN<32>, key: Bytes)` | `correlation::publish`; the first two topics are caller-supplied, `corr` is the reserved third element |

**Errors.**

`pause::PauseError` is the crate's only `#[contracterror]` enum. `errors.rs` deliberately contains **no** `#[contracterror]` — it declares a plain `#[repr(u32)] enum SharedErrorCode` (not a `contracterror`, therefore not part of any contract ABI) that maps every domain's error code into 100-wide bands, with a `description() -> &'static str` method, so SDK developers can decode any `u32` a Lumora contract returns.

| Variant | Code | Condition |
|---|---|---|
| `ContractPaused` | 1 | The contract is paused and the operation is not permitted. `require_not_paused_typed` / `try_unpause`; `require_not_paused` and `unpause` instead abort with the `Display` text |
| `RecoveryPending` | 2 | A recovery is scheduled but `env.ledger().sequence() < eta`; the time-lock holds |
| `RecoveryNotScheduled` | 3 | `cancel_recovery` called while `recovery_eta(env)` is `None` |
| `InvalidRecoveryDelay` | 4 | `delay_ledgers == 0`, above `MAX_RECOVERY_DELAY_LEDGERS` (120,960), or `ledger().sequence().checked_add(delay)` overflowed |
| `NotPaused` | 5 | `schedule_recovery` called while the contract is not paused |

`SharedErrorCode` (`contracts/shared/src/errors.rs`, not a `contracterror`) covers: General 1–99 — `Unauthorized = 1`, `NotFound = 2`, `AlreadyExists = 3`, `InvalidAmount = 4`, `InvalidStatus = 5`, `ContractPaused = 6`, `ArithmeticOverflow = 7`, `DeadlineInPast = 8`, `DeadlineTooFar = 9`, `InputTooLong = 10`, `HealthUnhealthy = 11`, `RolloutRolledBack = 12`, `InvalidAlertConfig = 13`, `InvalidCanaryBps = 14`, `RecoveryPending = 15`, `RecoveryNotScheduled = 16`, `InvalidRecoveryDelay = 17`, `ContractNotPaused = 18`; Escrow 100–112; Commission agreement 200–211; Campaign 300–306; Donation 400–406; Platform config 500–503; Dispute arbiter 600–603. The doc comment's band table lists Escrow as 100–199, agreement 200–299, campaign 300–399, donation 400–499, platform config 500–599, arbiter 600–699, but the highest variant actually defined in each band is well below the band ceiling.

**Storage.** See [STORAGE.md](./STORAGE.md#shared).

**Compile status.** `compiles` — `cargo check -p shared --lib` succeeds. Two documentation/behaviour gaps are present but do not affect compilation: the `upgrade` module doc advertises `require_upgrade_safe` and `export_storage_keys`, neither of which is defined anywhere in the crate (the implemented set is `record_upgrade`, `get_version`, `require_paused_for_upgrade`, `signal_migration_needed`), and the same doc calls the completion event `upgrade_complete` while the code publishes `symbol_short!("upgraded")`.


## Compile status summary

Five crates do not currently build. All five are pre-existing failures on
`upstream/main` at `f6abc91`, unrelated to this documentation, and are recorded
here rather than fixed because a parse error is a code change with its own review
burden:

| Crate | Failure |
|---|---|
| `reputation` | Unclosed delimiter. The file contains **two** `#[contract]` types spliced together with conflicting `DataKey`, `ReviewStatus` and `ReputationError` definitions, and two test modules targeting different clients. Only one contract can exist. |
| `platform_config` | Unclosed delimiter at `contracts/platform_config/src/lib.rs:256-261` (`resolution_cache`), masking three sibling files. |
| `escrow` | Mismatched closing/unclosed delimiter. The `autorls` event's `publish(` call is never closed. |
| `commission_agreement` | Unclosed delimiter. `DataKey::RateLimiter` and `types::RateLimitKey` are referenced but never declared; `AgreementError` has 30 variants across only 26 distinct discriminants. |
| `dao` | E0599 at `contracts/dao/src/lib.rs:352`: SEP-41's `token::Client` in soroban-sdk 21.7.7 has no `total_supply` method. |

The other 19 crates build cleanly. Sections above for a non-compiling crate
describe what is written in the source; where the parse error truncates the file
and the remainder could not be read, that is stated inline as `unknown —`.

## Known gaps in the current implementation

Noted during extraction, out of scope for this revision, and worth filing:

- `shared`'s upgrade module documents `require_upgrade_safe` and
  `export_storage_keys`, neither of which exists. The same doc calls the
  completion event `upgrade_complete`; the code publishes `upgraded`.
- Several contracts wrap `shared`'s unauthenticated setters with a
  caller-supplied `admin` parameter that is authorized but never compared against
  the stored admin, so any address can mutate health and rollout configuration.
  `audit` is the clearest example (`set_alert_config`, `report_ok`, `set_canary_deployment`).
- `AnalyticsError::Unauthorized` and `AuditError::Unauthorized` are never
  constructed: a non-admin fails at `require_auth()` with a host auth error long
  before the variant could be returned.
- `messaging` declares `MessagingError::MsgIdTooLong` and `MAX_MSG_ID_LEN`, but
  no call site can reach either.
- `commission_agreement`'s `update_contribution_note` contains a dead
  self-comparison branch, and its `tm_note` event omits the note from the payload.
- `search::initialize` has no `require_auth` at all, and `search` never extends
  TTL, so `DataKey::Listing` is never retained.
