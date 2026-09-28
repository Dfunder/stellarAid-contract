# Storage and TTL Management in stellarAid Contracts

Per-contract storage layout and TTL policy for every crate in the workspace.

> - **Retention and pruning policy:** [RETENTION_AND_PRUNING.md](./RETENTION_AND_PRUNING.md)
>   — what may be pruned, what must never be, the retention windows, and the
>   bounded prune entry point that exists today. Read this before adding any
>   `remove()` call.
> - **Query patterns and read costs:** [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md)
>   — the bounded-page convention every view entry point follows, and the
>   storage-layout observations behind it.
> - **Interfaces, events and errors:** [CONTRACTS.md](./CONTRACTS.md).
> - **Dependency supply chain:** §9 of this document.
> - **Design rationale:** [ADR-0007](./ADRs/0007-storage-data-model-and-ttl-management.md).

## How to read this

Soroban exposes three storage classes, and the choice is a cost decision rather
than a style one:

| Class | Cost | Scope | Used here for |
|---|---|---|---|
| `instance` | Cheapest. One entry per contract, tied to the contract's own instance TTL. | The contract as a whole | Admin address, pause flag, version, health counters, feature flags |
| `persistent` | Priced per entry, and **the entry can expire** without being touched. | Keyed records | Anything holding money, history, or an identity |
| `temporary` | Cheapest per-byte, but **restored at each restore point** and dropped on closeout. | Scratch space | Very little; see the per-contract tables |

The critical operational consequence: **a `persistent` entry that is written once
and never extended will expire**, and after it does, reads return "not found"
indistinguishably from a record that never existed. Any contract holding
valuable data must therefore either extend TTL on write, or accept expiry as a
designed retention window.

Every row below is transcribed from the source. `Storage class` is the
`env.storage().X()` call actually used at that site; `TTL` is the constant
actually passed to `extend_ttl`/`bump`, or an explicit statement that no TTL
management occurs.

## Summary across the workspace


| Contract | Storage classes | Ledger constants (ledgers) |
|---|---|---|
| [`analytics`](#analytics) | persistent, instance | `ANALYTICS_TTL_LEDGERS` 1,296,000, `MIN_RETENTION_LEDGERS` 1,296,000 |
| [`audit`](#audit) | persistent, instance | `AUDIT_TTL_LEDGERS` 5,184,000 |
| [`campaign`](#campaign) | persistent, instance | `MIN_TTL` 17,280, `MAX_TTL` 6,312,000 |
| [`commission_agreement`](#commission-agreement) | — | — none — |
| [`competitions`](#competitions) | persistent, instance | — none — |
| [`creator_fund`](#creator-fund) | persistent, instance | — none — |
| [`dao`](#dao) | persistent | `DEFAULT_TIMELOCK_LEDGERS` 17,280 |
| [`dispute_arbiter`](#dispute-arbiter) | persistent, instance | — none — |
| [`ecosystem_funding`](#ecosystem-funding) | persistent | — none — |
| [`escrow`](#escrow) | — | — none — |
| [`licensing`](#licensing) | persistent | — none — |
| [`mentorship`](#mentorship) | persistent | — none — |
| [`messaging`](#messaging) | persistent, instance | `RATE_LIMIT_LEDGERS` 12, `TYPING_EXPIRY_LEDGERS` 30 |
| [`multi_sig`](#multi-sig) | persistent, instance | `MULTISIG_TTL_LEDGERS` 1,296,000, `MAX_EXPIRY_LEDGERS` 120,960, `DEFAULT_EXPIRY_LEDGERS` 17,280 |
| [`nft`](#nft) | persistent | — none — |
| [`platform_config`](#platform-config) | persistent, instance | — none — |
| [`rate_limiter`](#rate-limiter) | persistent, instance | `DEFAULT_WINDOW_LEDGERS` 28,800 |
| [`recruitment`](#recruitment) | persistent, instance | — none — |
| [`reputation`](#reputation) | persistent, instance | `REPUTATION_TTL_LEDGERS` 1,296,000 |
| [`revenue_sharing`](#revenue-sharing) | persistent, instance | — none — |
| [`search`](#search) | persistent, instance | — none — |
| [`subscription`](#subscription) | persistent, instance | — none — |
| [`verification`](#verification) | persistent, instance | — none — |
| [`shared`](#shared) | instance | `DEFAULT_RECOVERY_DELAY_LEDGERS` 17,280, `MAX_RECOVERY_DELAY_LEDGERS` 120,960, `DEFAULT_STALL_LEDGERS` 17,280, `DEFAULT_ALERT_COOLDOWN_LEDGERS` 60, `SLA_HEALTH_CHECK_MAX_LEDGERS` 60 |

**Reading the ledger-constant column.** It lists every crate-level ledger
constant, which is not the same set as the TTLs. Several contracts have ledger
windows that are pure ledger-number comparisons and never touch `extend_ttl`:
`messaging`'s `RATE_LIMIT_LEDGERS` and `TYPING_EXPIRY_LEDGERS`,
`rate_limiter`'s `DEFAULT_WINDOW_LEDGERS`, `subscription`'s billing period and
grace window, `verification`'s badge lifetime and refresh interval, and
`dispute_arbiter`'s auto-resolve delay (which is a constructor argument, not a
constant at all). Each of those is annotated inline at its own table, so the
distinction survives even though the column heading is shared.

"— none —" means the crate declares no ledger constant *and* never calls
`extend_ttl`/`bump`. For a contract that stores nothing in `persistent` storage
that is correct and intentional. For one that does store persistent data, it
means the data sits at the network's minimum TTL and will expire unless
something else restores it — those cases are called out inline.

## Per-contract storage layout


### analytics

> Portfolio Analytics Contract — tracks artist performance metrics: earnings by category and client (#602), project completion rate, response time analytics, client satisfaction trends, and earnings predictions (rolling average). Closes #602.

Source: `contracts/analytics/`

**Storage layout.**

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none — no `extend_ttl` on this key | Platform oracle; sole writer of analytics data. Written once by `initialize`; **not** rotatable afterwards |
| `DataKey::Metrics(Address)` | `ArtistMetrics` | persistent | `ANALYTICS_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `save_metrics` | Per-artist aggregate: totals, counts, response-time and satisfaction sums, `last_updated_ledger`. Never pruned |
| `DataKey::Earning(Address, u32)` | `EarningsRecord` | persistent | `ANALYTICS_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `record_earning` | Append-only earnings log keyed by `(artist, index)`; the only key `prune_earnings` may `remove` |
| `DataKey::EarningCount(Address)` | `u32` | persistent | `ANALYTICS_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `record_earning` | Monotonic count/next index; deliberately **not** decremented by `prune_earnings` |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `ANALYTICS_TTL_LEDGERS` | 1,296,000 | ~90 days (source comment: "~90 days at 6 s/ledger"; the same value is 75 days at the 5 s rate) | Retention window and `extend_ttl` amount for `DataKey::Metrics`, `DataKey::Earning`, `DataKey::EarningCount` |
| `MIN_RETENTION_LEDGERS` | 1,296,000 | ~90 days (same as above; defined as `ANALYTICS_TTL_LEDGERS`) | Minimum age before `prune_earnings` will touch a record; also the pruning cutoff, `ledger() - MIN_RETENTION_LEDGERS` |

Not TTLs but hard read/write bounds: `MAX_EARNING_PAGE = 50` (cap on `get_earnings` page size) and `PRUNE_MAX_BATCH = 100` (cap on records removed *and* indexes scanned per `prune_earnings` call).

**Compile status.** `compiles` — `cargo check -p analytics --lib` succeeds.


### audit

> Transaction History and Audit Log Contract — closes #712, with immutability achieved structurally (append-only under a monotonic sequence number, no `update_entry`/`delete_entry`/`clear` anywhere), all reads bounded by `MAX_PAGE_SIZE`/`MAX_QUERY_SCAN`, and no free text, memos, names, or identifiers of any kind in the stored schema.

Source: `contracts/audit/`

**Storage layout.**

Keys from `storage::DataKey` (re-exported as `StorageKey`), plus the `shared` instance keys reached through the health/rollout entry points.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Platform admin; the only address that may attest transactions. Written once by `initialize`, no rotation path |
| `DataKey::Sequence` | `u32` | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `bump_count` → `renew` | The append counter: next sequence to hand out, and simultaneously the total append count. **The single mutable value in the contract** |
| `DataKey::Entry(u32)` | `AuditEntry` | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `renew(entry_key)` | The immutable log line, keyed by its never-reused sequence number |
| `DataKey::ByReference(Bytes, u32)` | `u32` (sequence) | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `renew(index_ref)` | Per-reference status trail, oldest first |
| `DataKey::ReferenceCount(Bytes)` | `u32` | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `bump_count` | Appends recorded for a reference |
| `DataKey::ByAccount(Address, u32)` | `u32` (sequence) | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `renew` in `push_account` | Per-account trail; an entry is indexed under `from` always and under `to` only when it differs, so no account sees the same entry twice |
| `DataKey::AccountCount(Address)` | `u32` | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `bump_account_count` → `bump_count` | Appends an account appears in |
| `DataKey::ByStatus(TxStatus, u32)` | `u32` (sequence) | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `renew(status_index)` | Per-status trail |
| `DataKey::StatusCount(TxStatus)` | `u32` | persistent | `AUDIT_TTL_LEDGERS` (5,184,000) via `bump_count` | Appends carrying a status |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Shared pause flag, read via `health` and set by `trigger_rollback` |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | Success/error counters behind `health_check` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Alert thresholds |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | `hlth_alrt` cooldown anchor |
| `shared::rollout::RolloutKey::Phase` / `CanaryBps` / `Canary` / `Stable` / `RollbackErrorBps` / `Flag(Symbol)` / `FlagIndex` | `RolloutPhase` / `u32` / `Address` / `Address` / `u32` / `bool` / `Vec<Symbol>` | instance | none | Shared canary and feature-flag state |
| `shared::upgrade::UpgradeKey::Version` / `LastUpgradeLedger` | `ContractVersion` / `u32` | instance | none | **Declared in `shared` but never read or written by `audit`** — no upgrade entry point is exposed |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `AUDIT_TTL_LEDGERS` | 5,184,000 | ~300 days (source comment: "~300 days at 5 s/ledger") | Every `persistent()` key written by `storage::write_entry` and its helpers — `Entry`, `ByReference`, `ReferenceCount`, `ByAccount`, `AccountCount`, `ByStatus`, `StatusCount`, `Sequence`. Deliberately the longest TTL in the workspace because "an audit trail that silently expires is not an audit trail" |

Query bounds, not TTLs: `MAX_PAGE_SIZE = 50` and `MAX_QUERY_SCAN = 200`. No `extend_ttl` or `bump` is applied to the instance-storage keys (admin, pause, health, rollout).

**Compile status.** `compiles` — `cargo check -p audit --lib` succeeds. Two behavioural notes rather than build errors: the 19 `shared`-backed entry points authorize a caller-supplied `admin` parameter without comparing it to the stored `DataKey::Admin`, so any address can mutate health/rollout configuration; and `AuditError::Unauthorized` is unreachable in the current implementation.


### campaign

> unknown — `contracts/campaign/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc comment; the summary below is reconstructed from the source (and matches `docs/CONTRACTS.md`: "Manages fundraising campaign lifecycle"). `src/invariant_tests.rs` does carry a `//!` doc, but it describes the test invariants, not the contract.

Source: `contracts/campaign/`

**Storage layout.**

Seven keys are the crate's own `DataKey` enum; the rest are written into this contract's own instance storage by the `shared` helpers it delegates to (`shared::pause`, `shared::health`, `shared::rollout`, `shared::version`). `DataKey::Campaign(u64)` is the only persistent entry and the only one that receives a TTL bump.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | The single platform admin; set by `initialize`, replaced by `set_admin` / `transfer_admin`; every admin-only path reads it |
| `DataKey::Initialized` | `bool` | instance | none | Re-entrancy guard for `initialize`; always written as `true`, checked with `has` |
| `DataKey::CampaignCount` | `u64` | instance | none | Monotonic campaign-ID counter *and* the value returned by `get_campaign_count`; pre-incremented in `next_campaign_id` |
| `DataKey::Frozen` | `bool` | instance | none | Hard freeze switch; `true` makes every `require_not_frozen` call site abort with `ContractFrozen` |
| `DataKey::UnderReview` | `bool` | instance | none | Fraud-review flag; `true` makes `finalize_withdrawal` abort with `UnderReview` (and nothing else) |
| `DataKey::ReviewReason` | `BytesN<32>` | instance | none | 32-byte reason hash stored by `flag_for_review`, `remove`d by `clear_review_flag`; read by `get_review_reason` |
| `DataKey::Campaign(u64)` | `shared::types::Campaign` | persistent | `extend_ttl(MIN_TTL, MAX_TTL)` on every write | The campaign record: `{id, owner, goal, raised, status, deadline, fee_bps, platform_wallet}` |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Emergency-pause flag, distinct from `DataKey::Frozen`; written by `pause`/`unpause` and by `trigger_rollback` |
| `shared::pause::PauseDataKey::RecoveryEta` | `u32` | instance | none | Time-lock ledger for `unpause`; this contract exposes no `schedule_recovery`, so it stays at the `0` sentinel and never blocks `unpause` |
| `shared::upgrade::UpgradeKey::Version` | `shared::upgrade::ContractVersion` | instance | none | On-chain semver, seeded from `CARGO_PKG_VERSION` by `initialize`; falls back to the crate version when unset |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | `{ok_count, error_count, last_ok_ledger, last_error_ledger, paused}`; only ever written by `report_ok` / `report_error` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Degraded/unhealthy bps, stall window, alert cooldown, `alerting_enabled` |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | Cooldown anchor that rate-limits `hlth_alrt` events |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | none | `Off` / `Canary` / `Full` / `RolledBack` |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | none | Share of callers routed to the canary (0–10000 bps) |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | none | Canary contract id |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | none | Stable contract id |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | none | Error rate at which automatic rollback arms |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | none | One entry per named feature flag |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates the flags so a rollback can zero them all |
| `shared::upgrade::UpgradeKey::LastUpgradeLedger` | — | instance | n/a | **Never written.** `upgrade` calls `env.deployer().update_current_contract_wasm` directly instead of `shared::upgrade::record_upgrade` |

Not part of the compiled contract: `src/asset_helpers.rs`, `src/lifecycle_helpers.rs` and `src/refund_helpers.rs` are **not declared as modules** in `lib.rs` (only `invariant_tests` and the inline `test` module are), so they are dead files that never reach the WASM. They would have used `env.storage().persistent()` under ad-hoc tuple keys `(symbol_short!("bal"), participant, asset)`, `(symbol_short!("status"), campaign_id)`, and with a hand-rolled status encoding (`0` Active, `1` Ended, `2` Refunded, `3` Cancelled) that contradicts `shared::types::CampaignStatus`. Wiring them in as-is would also clash with the `DataKey` enum.

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `MIN_TTL` | 17,280 | 1 day (source comment: "1 day in ledgers (assuming 5s ledger time)") | Threshold argument to `extend_ttl` on `DataKey::Campaign(u64)` in `bump_campaign_ttl` |
| `MAX_TTL` | 6,312,000 | 1 year (source comment: "1 year in ledgers (assuming 5s ledger time)") | `extend_ttl` target for `DataKey::Campaign(u64)` in `bump_campaign_ttl` |

`MIN_TTL` / `MAX_TTL` are the only `extend_ttl` arguments in the crate, and the only place either is used is the public `bump_campaign_ttl`, which `create_campaign`, `update_raised` and `finalize_withdrawal` call after each persistent write. Every instance-storage entry above is never extended. Two further constants are *not* TTLs but bound the same persistent record's useful life: `MAX_DEADLINE_OFFSET_SECS = 63_115_200` seconds (~2 years, per its doc comment; closes #592) caps how far into the future a `deadline` may be set, and `MAX_STRING_INPUT_LEN = 512` bytes caps `reason` in `reject_campaign` (closes #591).

**Compile status.** `compiles` — `cargo check -p campaign` and `cargo test -p campaign --no-run` both succeed with no warnings. The 16 unit tests in the inline `#[cfg(test)] mod test` plus the 8 in `src/invariant_tests.rs` compile. It *is* a workspace member despite the stale `NOTE` in the root `Cargo.toml`.


### commission_agreement

> CommissionAgreement contract — core agreement lifecycle functions.

Source: `contracts/commission_agreement/`

**Storage layout.**

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Agreement(Bytes)` | `AgreementRecord` | `persistent` | none — no `extend_ttl` call in the crate | The agreement itself. Read/written by `load_agreement`, `create_agreement`, `accept_agreement`, `reject_agreement`, `approve_milestone`, `invite_team_member`, `update_contribution_note`, `get_team_members` (existence check), `get_milestones` (existence check), `cancel_agreement`, `respond_to_revision`, and `attribute_commission`'s sibling calls. `budget_usdc` is mutated by `respond_to_revision`. |
| `DataKey::Milestone(Bytes, Bytes)` | `MilestoneRecord` | `persistent` | none | One milestone, keyed `(commission_id, milestone_id)`. Set by `propose_milestone`; read and re-set to `Approved` by `approve_milestone`. |
| `DataKey::MilestonesForAgreement(Bytes)` | `Vec<MilestoneRecord>` | `persistent` | none | The ordered per-agreement milestone list. Written to an empty `Vec` by `create_agreement`, appended to by `propose_milestone`, and rebuilt entry-by-entry by `approve_milestone` so the all-approved check reads accurate data. |
| `DataKey::TeamMembers(Bytes)` | `Vec<TeamMember>` | `persistent` | none | Team roster for one agreement, with `TeamRole`, `InvitationStatus`, `payment_share_bps` and `contribution_note`. Mutated by `invite_team_member`, `accept_team_invitation`, `decline_team_invitation`, `update_contribution_note`; read by `get_team_members`. |
| `DataKey::MilestoneLock(Bytes, Bytes)` | `bool` (`true`) | `persistent` | none | Serialization lock for milestone transitions (#589), keyed `(commission_id, milestone_id)`. Set before the read in `approve_milestone` and `remove`d on the normal path and on the `InvalidStatus` early return. |
| `DataKey::CancellationPolicy(Bytes)` | `CancellationPolicy` | `persistent` | none | Per-agreement `penalty_bps` + `grace_ledgers`. Set by `set_cancellation_policy` (client-auth, `Pending` only); read via `load_policy`, falling back to `cancellation::default_policy` when absent. |
| `DataKey::Cancellation(Bytes)` | `CancellationRecord` | `persistent` | none | The settlement produced for one cancelled agreement. Written by `cancel_agreement`, read by `get_cancellation`. |
| `DataKey::CancellationHistory` | `Vec<CancellationRecord>` | `persistent` | none | Global, contract-wide ring of recent cancellations. `cancel_agreement` pops the front while `len() >= CANCELLATION_HISTORY_LIMIT` (50) then pushes back; read by `get_cancellation_history`. Not keyed by agreement. |
| `DataKey::Agency(Address)` | `AgencyProfile` | `persistent` | none | A registered agency's own identity, `default_split_bps` and live `artist_count`. Written by `register_agency`, `add_artist`, `remove_artist`; read by `load_agency` / `get_agency`. |
| `DataKey::Roster(Address)` | `Vec<Address>` | `persistent` | none | The agency's artist list. Initialized to an empty `Vec` by `register_agency`, appended by `add_artist`, filtered by `remove_artist`; read by `get_roster`. |
| `DataKey::RosterEntry(Address, Address)` | `RosterEntry` | `persistent` | none | Per-artist roster record keyed `(agency, artist)`: `split_bps`, `commissions`, `gross_distributed`, `agency_revenue`, `artist_payouts`. Written by `add_artist`, `set_artist_split`, `distribute_batch`, and `attribute_commission`; read by `load_roster_entry` / `get_roster_entry`. |
| `DataKey::ArtistAgency(Address)` | `Address` | `persistent` | none | Reverse index artist → representing agency, enforcing one agency per artist. Set by `add_artist`, `remove`d by `remove_artist`; read by `get_artist_agency` and by `attribute_commission` to auto-credit commission attribution. |
| `DataKey::AgencyAnalytics(Address)` | `AgencyAnalytics` | `persistent` | none | Running totals per agency (`artist_count`, `commissions`, `commission_budget`, `batches`, `gross_distributed`, `agency_revenue`, `artist_payouts`) kept O(1) rather than recomputed. Written by `save_analytics` from `add_artist`, `remove_artist`, `distribute_batch` and `attribute_commission`; read by `get_agency_analytics`. |
| `DataKey::RevisionPolicy(Bytes)` | `RevisionPolicy` | `persistent` | none | Per-agreement revision cap. Set by `set_revision_policy` (client-auth, `Pending` only); read via `load_revision_policy`, falling back to `revision::default_policy` (`DEFAULT_MAX_REVISIONS = 5`). |
| `DataKey::RevisionsForAgreement(Bytes)` | `Vec<RevisionRequest>` | `persistent` | none | The bounded revision history for one agreement. Appended by `request_revision`, index-replaced by `respond_to_revision`; read by `get_revisions`, `get_revision`, `get_revision_count`. |
| `DataKey::RateLimiter` | `Address` | `instance` | none | Address of the external rate-limiter contract invoked at the top of `create_agreement`. **This variant is not declared in the `DataKey` enum in `src/types.rs`** — it is referenced only at `src/lib.rs:215`. Would be a compile error if the parse error were fixed; the whole line is unreachable today. |
| `HealthKey::Metrics` (in `shared`, key enum owned by `contracts/shared/src/health.rs`) | `HealthMetrics` | `instance` | none | Written by `shared::health::record_ok` / `record_error` / `get_metrics`, reached through this contract's `report_ok`, `report_error`, `get_health_metrics`, `health_check`, `detect_anomaly`. |
| `HealthKey::*` alert-config key (in `shared`) | `AlertConfig` | `instance` | none | Reached through this contract's `set_alert_config` / `get_alert_config`. Exact variant name and key shape: unknown — the `HealthKey` enum is declared in `contracts/shared/src/health.rs`, outside this crate's source tree, and was not read for this fragment. |
| `RolloutKey::Canary`, `RolloutKey::Stable`, `RolloutKey::Phase` (in `shared`) | `Address`, `Address`, `RolloutPhase` | `instance` | none | Canary/stable endpoints and rollout phase, read by `route_to_canary` / `get_rollout_state` and written by `set_canary_deployment` and `maybe_auto_rollback`. |
| `RolloutKey::*` feature-flag and rollback-trigger keys (in `shared`) | unknown — value type not determined | `instance` | none | Reached through `set_feature_flag`, `is_feature_enabled`, `set_rollback_trigger`, `should_rollback`, `trigger_rollback`. Exact `RolloutKey` variant names beyond `Canary`/`Stable`/`Phase`: unknown — declared in `contracts/shared/src/rollout.rs`, outside this crate's source tree, and not read for this fragment. |

No `env.storage().temporary()` call exists anywhere in the crate.

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none — this crate defines no TTL constant and calls no `extend_ttl` / `bump` | — | — | No entry written by `commission_agreement` is ever TTL-extended. Every `persistent()` and `instance()` write here relies solely on the network's default minimum entry TTL, so a dormant agreement, roster entry or cancellation record can expire and become unreadable. |

The only ledger-denominated constant in the crate is not a TTL:

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `MAX_DEADLINE_OFFSET_LEDGERS` (`src/lib.rs:180`) | `12_614_400` | ≈ 2 years (source comment: "At ~5 s per ledger: 12_614_400 ≈ 2 years") | Upper bound on how far into the future `create_agreement` and `request_revision` may set `deadline_ledger`; exceeding it returns `DeadlineTooFar`. |

`CancellationPolicy.grace_ledgers` is a per-agreement free-cancellation window in ledgers, but it is stored data, not a compile-time constant — its value is supplied by the client at `set_cancellation_policy` time (`cancellation_tests.rs` uses 50 and 100).

**Compile status.** Does not compile. `cargo build -p commission_agreement` fails with `error: this file contains an unclosed delimiter`, anchored at `src/lib.rs:1335:23` (the `mod integration_tests;` token), with the compiler pointing at the unclosed `{` of the `#[contractimpl] impl CommissionAgreementContract` block at `src/lib.rs:186` and identifying the `{` that opens `get_team_members`' body at `src/lib.rs:678` as the delimiter "that might not be properly closed"; the `}` at `src/lib.rs:1324` is the candidate it matches, and it is consumed as that function's missing brace. Net effect: everything from `src/lib.rs:685` to `src/lib.rs:1323` — 41 `pub fn` entry points covering cancellation, revisions, the agency layer, and all `shared` health/rollout wrappers — is parsed as statements inside `get_team_members`' body rather than as sibling contract functions, and the `#[contractimpl]` block is never closed. Additionally, three latent errors are masked behind the parse failure and would surface next: the duplicate `use types::{…}` at `src/lib.rs:37-38` and `src/lib.rs:54`; the reference to the undeclared `DataKey::RateLimiter` variant at `src/lib.rs:215`; and the reference to the non-existent `types::RateLimitKey::CommissionsPerArtist` at `src/lib.rs:222`. Separately, `src/milestone_flow.rs`, `src/multiple_escrows.rs`, `src/dispute_resolution.rs` and `src/test.rs` are all gated behind the non-default `legacy_tests` feature and are independently broken (they import `crate::test::{helpers, test_lifecycles}` and `crate::types::{Client, Commission, Milestone}`, none of which exist), so re-enabling that feature fails too. `src/integration_tests.rs` is never compiled because the `mod` statement that would pull it in sits outside the unclosed block. The tables above document what is *written* in the source; because of the parse error, no public function past `get_team_members` is actually reachable as a contract entry point, and the code from `src/lib.rs:685` onward is documentation of intent rather than of working behaviour. Not fixed, as instructed.


### competitions

> unknown — `contracts/competitions/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc; `src/types.rs` and `src/errors.rs` have none either. The summary below is reconstructed from the source (contracts/competitions, closes no numbered issue; `MAX_PRIZE_POSITIONS`/`TOTAL_BPS` and the rank-and-payout flow are the contract's substance).

Source: `contracts/competitions/`

**Storage layout.**

`DataKey` (in `src/types.rs`) holds the two instance keys; every competition-scoped key is persistent. This contract does **not** use `shared::pause` or `shared::version` — its `get_version` family parses `CARGO_PKG_VERSION` at runtime and touches no storage, and there is no `Paused` key of its own (one is still created, indirectly, by `trigger_rollback`).

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Set by `initialize`; read only by `require_admin` (i.e. by `set_reputation`). Also doubles as the "is initialized" flag via `has_admin` |
| `DataKey::HistoryLimit` | `u32` | instance | none | Maximum length of the `History` deque; must be non-zero at `initialize` |
| `DataKey::Competition(Bytes)` | `Competition` | persistent | none | Full record: `{id, organizer, token, title, prize_pool, rules, status, submission_end_ledger, voting_end_ledger, submission_count, total_votes, created_ledger}` |
| `DataKey::Entrants(Bytes)` | `Vec<Address>` | persistent | none | Submission-ordered entrant list; also the tie-break order in `rank_entries` |
| `DataKey::Submission(Bytes, Address)` | `Submission` | persistent | none | `{competition_id, entrant, entry_uri, votes, voter_count, submitted_ledger}`; one entry per entrant per competition |
| `DataKey::Voted(Bytes, Address)` | `Address` | persistent | none | The entrant this voter chose; its presence is the one-ballot-per-voter guard. The stored value is otherwise unused |
| `DataKey::Winners(Bytes)` | `Vec<Winner>` | persistent | none | Final ranking written by `finalize`, read by `distribute_prizes` and `get_winners` |
| `DataKey::Reputation(Address)` | `u32` | persistent | none | Off-chain-computed, admin-attested ballot weight per account; `0` means no reputation |
| `DataKey::History` | `Vec<CompetitionSummary>` | persistent | none | Capped ring of finalized-competition summaries; `pop_front()`-trimmed to `HistoryLimit` on every `finalize` |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | `{ok_count, error_count, last_ok_ledger, last_error_ledger, paused}` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Degraded/unhealthy bps, stall window, alert cooldown, `alerting_enabled` |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | Cooldown anchor for `hlth_alrt` |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | none | `Off` / `Canary` / `Full` / `RolledBack` |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | none | Share of callers routed to canary (0–10000 bps) |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | none | Canary contract id |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | none | Stable contract id |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | none | Error rate at which automatic rollback arms |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | none | One entry per named feature flag |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates flags for rollback |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Never set by this contract's own code; written `true` by `trigger_rollback` and read back by `health::get_metrics` to report `metrics.paused` |

**TTL constants.**

None. The crate declares no `const` ledger figure of any kind — not for TTL and not for windows (unlike `campaign`, which has `MIN_TTL`/`MAX_TTL`), and it never calls `extend_ttl`, `bump`, or `extend_ttl` on any key. The `CompetitionRules::submission_ledgers` / `voting_ledgers` fields are *per-competition parameters supplied by the organizer at `create_competition`*, not constants in the source. Every instance and persistent entry above is therefore governed by the contract's own storage-expiration configuration with no on-chain bump.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none | — | — | No TTL constant in the crate and no `extend_ttl`/`bump` call at any storage site |

**Compile status.** `compiles` — `cargo check -p competitions` and `cargo test -p competitions --no-run` both succeed with no warnings. The 22 unit tests in `src/test.rs` compile against the `rlib` target.


### creator_fund

> unknown — `contracts/creator_fund/src/lib.rs` opens with `#![no_std]` and has no crate-level `//!` module doc; `src/types.rs` and `src/errors.rs` have none either. The summary below is reconstructed from the source (contributor-weighted capital pools with steward-configured distribution rules, closes no numbered issue).

Source: `contracts/creator_fund/`

**Storage layout.**

Two instance keys; everything fund-scoped is persistent. There is no `shared::pause` or `shared::version` usage, so no `Paused` key of its own and no stored semver.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Written by `initialize` and then read **only** by `has_admin`, as an "is initialized" flag. This contract has no admin-gated function and no `require_admin`, so the stored admin address gates nothing |
| `DataKey::HistoryLimit` | `u32` | instance | none | Caps both the `Growth` and `Allocations` deques; must be non-zero at `initialize` |
| `DataKey::Fund(Bytes)` | `Fund` | persistent | none | `{id, fund_type, steward, token, balance, total_contributed, total_allocated, contributor_count, rule, created_ledger}` |
| `DataKey::Contribution(Bytes, Address)` | `i128` | persistent | none | Per-account contributed capital — the voting-power source. `0` means "not a contributor" |
| `DataKey::Growth(Bytes)` | `Vec<GrowthPoint>` | persistent | none | `{ledger, balance, total_contributed, total_allocated}` snapshots on create, contribute and execute; trimmed to `HistoryLimit` |
| `DataKey::Proposal(Bytes)` | `Proposal` | persistent | none | `{id, fund_id, proposer, recipient, amount, status, votes_for, votes_against, created_ledger, voting_ends_ledger, memo}` |
| `DataKey::Allocations(Bytes)` | `Vec<Allocation>` | persistent | none | `{proposal_id, recipient, amount, ledger}` payouts; trimmed to `HistoryLimit` |
| `DataKey::Voted(Bytes, Address)` | `bool` | persistent | none | The vote this contributor cast; presence is the one-vote guard, value is the `support` flag |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | `{ok_count, error_count, last_ok_ledger, last_error_ledger, paused}` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Degraded/unhealthy bps, stall window, alert cooldown, `alerting_enabled` |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | Cooldown anchor for `hlth_alrt` |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | none | `Off` / `Canary` / `Full` / `RolledBack` |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | none | Canary traffic share (0–10000 bps) |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | none | Canary contract id |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | none | Stable contract id |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | none | Error rate at which automatic rollback arms |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | none | One entry per named feature flag |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates flags for rollback |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Never set by this contract's own code; written `true` by `trigger_rollback` and read back by `health::get_metrics` |

**TTL constants.**

None. The crate declares no `const` ledger figure — `DistributionRule::voting_ledgers` is a per-fund parameter supplied by the steward at `create_fund`/`set_rule`, not a source constant — and no storage site calls `extend_ttl` or `bump`. Both the instance and the persistent entries are therefore left to the contract's own storage-expiration configuration with no on-chain extension.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none | — | — | No TTL constant in the crate and no `extend_ttl`/`bump` call at any storage site |

**Compile status.** `compiles` — `cargo check -p creator_fund` and `cargo test -p creator_fund --no-run` both succeed with no warnings. The 17 unit tests in `src/test.rs` compile against the `rlib` target.


### dao

> This crate has no `//!` module doc comment. Nearest header comment (contracts/dao/src/lib.rs:1-9): "Implements DAO Governance Contract — closes #615. Acceptance Criteria: Implement voting mechanism (1 token = 1 vote); Support proposal creation and discussion; Add timelock for execution; Implement multi-sig validation; Track governance history."

Source: `contracts/dao/`

**Storage layout.**

Every site in this crate uses `env.storage().persistent()`; there is no `instance()` or `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the file.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | The single governance admin: may cancel any `Active` proposal, is the only address allowed to `execute`, and is recorded as the `actor` of `tally` history entries. Set once in `initialize`; never rotated. |
| `DataKey::GovToken` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | Address of the SEP-41 governance token contract. Read in `create_proposal`, `cast_vote`, and `tally` to derive vote weights and total supply. |
| `DataKey::MultiSigSigners` | `Vec<Address>` | persistent | unset — no `extend_ttl`/`bump` in the crate | The list of addresses permitted to call `approve_multisig`. Membership is checked with `signers.contains(&signer)`. |
| `DataKey::MultiSigThreshold` | `u32` | persistent | unset — no `extend_ttl`/`bump` in the crate | Minimum number of multi-sig approvals required before a `requires_multisig` proposal can advance. Asserted `<= multisig_signers.len()` at init; `unwrap_or(1)` on read. |
| `DataKey::TimelockLedgers` | `u32` | persistent | unset — no `extend_ttl`/`bump` in the crate | Execution delay in ledgers. `initialize` substitutes `DEFAULT_TIMELOCK_LEDGERS` when the caller passes `0`; `tally` uses it (falling back to `DEFAULT_TIMELOCK_LEDGERS`) to set `executable_after`. |
| `DataKey::VotingPeriodLedgers` | `u32` | persistent | unset — no `extend_ttl`/`bump` in the crate | Voting window length in ledgers. `initialize` asserts `> 0`; `create_proposal` reads it to compute `voting_end = now + voting_period`. |
| `DataKey::ProposalCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic proposal-id counter, initialised to `0`; `next_proposal_id` (internal) returns `count + 1` and persists it. |
| `DataKey::Proposal(id)` | `Proposal` | persistent | unset — no `extend_ttl`/`bump` in the crate | Full proposal record: `id, title, description, proposer, votes_for, votes_against, status, voting_start, voting_end, executable_after, requires_multisig, multisig_approvals`. Written on create, cancel, vote, tally, multisig-approve, and execute. |
| `DataKey::HasVoted(proposal_id, voter)` | `bool` | persistent | unset — no `extend_ttl`/`bump` in the crate | Double-vote guard, keyed `(u64, Address)`. Read with `unwrap_or(false)`; set to `true` in `cast_vote`. |
| `DataKey::Vote(proposal_id, voter)` | `VoteRecord` | persistent | unset — no `extend_ttl`/`bump` in the crate | Per-voter vote receipt: `proposal_id, voter, weight, support, ledger`. Written in `cast_vote`, read by `get_vote`. |
| `DataKey::MultiSigApproval(proposal_id, signer)` | `bool` | persistent | unset — no `extend_ttl`/`bump` in the crate | Double-approval guard, keyed `(u64, Address)`. Read with `unwrap_or(false)`, set to `true` in `approve_multisig`. |
| `DataKey::HistoryCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic index into the governance history log, initialised to `0` in `initialize`. `record_history` (internal) increments it on every state transition. |
| `DataKey::History(index)` | `GovernanceEvent` | persistent | unset — no `extend_ttl`/`bump` in the crate | Append-only history entry: `proposal_id, action, actor, ledger`. `action` is one of the string literals `"created"`, `"cancelled"`, `"queued"`, `"defeated"`, `"multisig_approved"`, `"executed"`. Read by `get_history`. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `DEFAULT_TIMELOCK_LEDGERS` | 17,280 | ~1 day (source comment: "~5 seconds/ledger → 1 day ≈ 17280 ledgers") | Substituted for `DataKey::TimelockLedgers` when `initialize` is passed `0`; also the `unwrap_or` fallback in `tally`. Determines the `Queued → Executed` delay. |
| `QUORUM_BPS` | n/a — not a ledger value | n/a | Not a TTL. It is a basis-point numerator: `quorum = (total_supply * QUORUM_BPS) / 10_000` in `tally`, i.e. 10% of total governance-token supply. |

No other ledger-denominated constants exist. `voting_period` and `timelock_ledgers` are runtime constructor parameters, not constants. The crate never extends any entry's TTL.

**Compile status.** `fails to compile`  > **Compile status:** `cargo build -p dao` fails with exactly one error: `error[E0599]: no method named 'total_supply' found for struct 'Client<'a>' in the current scope` at `contracts/dao/src/lib.rs:352:32` (`let total_supply = tok.total_supply();`, inside `tally`). The `tok` value is a `soroban_sdk::token::Client` (alias of `TokenClient`, generated from the SEP-41 `TokenInterface` trait in soroban-sdk 21.7.7), which exposes only `allowance`, `approve`, `balance`, `transfer`, `transfer_from`, `burn`, `burn_from`, `decimals`, `name`, and `symbol` — SEP-41 defines no `total_supply` function, so the call cannot be satisfied. Consequently: the entire crate produces no `cdylib`/`rlib` artifact, so every other function (`initialize`, `create_proposal`, `cancel_proposal`, `cast_vote`, `approve_multisig`, `execute`, `get_proposal`, `has_voted`, `get_vote`, `get_history`) is unreachable in practice and no `DaoGovernanceClient` or WASM can be built or deployed. Within `tally` specifically, everything downstream of line 352 is unreachable: the quorum computation `quorum = (total_supply * QUORUM_BPS) / 10_000`, the `passed` predicate, the `Active → Queued` and `Active → Defeated` transitions, the assignment `executable_after = now + timelock`, the final `Proposal` write, the `"queued"`/`"defeated"` history record, and the `tallied` event. Because `tally` is the *only* writer of `executable_after`, the whole downstream governance path is dead: `approve_multisig` can never observe `status == Queued` (so the multi-sig path and the `Queued → Ready` transition are unreachable), and `execute`'s `now >= executable_after` check can only ever be satisfied against the `0` written at creation — reachable in principle for a non-`requires_multisig` proposal, but unreachable in practice since no artifact builds. There is no test module in this crate, so no test failure masks the build error.


### dispute_arbiter

> "Dispute Arbiter Smart Contract — Autonomous arbitration and dispute settlement for StellarAid escrows. Architecture Decision: [ADR-0004](../../docs/ADRs/0004-dispute-resolution-and-arbitration.md)"

Source: `contracts/dispute_arbiter/`

**Storage layout.**

Configuration lives in `instance()` storage; per-dispute records live in `persistent()` storage. No `temporary()` access, and no `extend_ttl`/`bump` call in this crate.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | unset — no `extend_ttl`/`bump` in this crate; entry lives and dies with the contract instance | The arbiter admin. Used as the "is initialised" sentinel (`has_admin`), and `admin.require_auth()` in `resolve_for_client`, `resolve_for_artist`, and `partial_resolve`. |
| `DataKey::EscrowContract` | `Address` | instance | unset — no `extend_ttl`/`bump` in this crate | Target of every `env.invoke_contract` to the escrow: `refund_cl` and `rel_pay`. |
| `DataKey::ConfigContract` | `Address` | instance | unset — no `extend_ttl`/`bump` in this crate | Passed through to the escrow on every settlement call, and invoked directly for `get_usdc` in `partial_resolve`. |
| `DataKey::AutoResolveLedgers` | `u32` | instance | unset — no `extend_ttl`/`bump` in this crate | Constructor-supplied delay; `open_dispute` computes `auto_resolve_ledger = opened_ledger + auto_resolve_ledgers`. No crate-level constant for the default — it is whatever the deployer passes. |
| `DataKey::Dispute(commission_id)` | `DisputeRecord` | persistent | unset — no `extend_ttl`/`bump` in this crate | The dispute record: `commission_id: Bytes, opened_ledger: u32, auto_resolve_ledger: u32, status: DisputeStatus, resolution_note: Option<String>`. Existence is keyed by `commission_id`, so a commission can hold at most one dispute. |
| `HealthKey::Metrics` (`shared::health`) | `HealthMetrics` | instance | unset — no `extend_ttl`/`bump` in this crate | `ok_count`, `error_count`, `last_ok_ledger`, `last_error_ledger`, `paused`. Written by `report_ok` / `report_error`; read by `get_health_metrics`, `health_check`, `detect_anomaly`, `should_rollback`. |
| `HealthKey::AlertConfig` (`shared::health`) | `AlertConfig` | instance | unset — no `extend_ttl`/`bump` in this crate | Alert thresholds (`degraded_error_bps`, `unhealthy_error_bps`, `stall_ledgers`, `alert_cooldown_ledgers`, `alerting_enabled`). Set by `set_alert_config`, defaulted by `get_alert_config`. |
| `HealthKey::LastAlertLedger` (`shared::health`) | `u32` | instance | unset — no `extend_ttl`/`bump` in this crate | Cooldown marker for `hlth_alrt` emission; written only from `health_check`. |
| `PauseDataKey::Paused` (`shared::pause`) | `bool` | instance | unset — no `extend_ttl`/`bump` in this crate | Pause flag surfaced via `HealthMetrics.paused` and set to `true` by `trigger_rollback`. Note: this crate exposes no un-pause entry point. |
| `RolloutKey::Phase` (`shared::rollout`) | `RolloutPhase` | instance | unset — no `extend_ttl`/`bump` in this crate | `Off` / `Canary` / `Full` / `RolledBack`. Gates `route_to_canary` and `is_feature_enabled`. |
| `RolloutKey::CanaryBps` (`shared::rollout`) | `u32` | instance | unset — no `extend_ttl`/`bump` in this crate | Share of traffic routed to the canary, in bps of 10 000. |
| `RolloutKey::Canary` (`shared::rollout`) | `Address` | instance | unset — no `extend_ttl`/`bump` in this crate | Canary contract id, set by `set_canary_deployment`. |
| `RolloutKey::Stable` (`shared::rollout`) | `Address` | instance | unset — no `extend_ttl`/`bump` in this crate | Stable contract id, set by `set_canary_deployment`. |
| `RolloutKey::RollbackErrorBps` (`shared::rollout`) | `u32` | instance | unset — no `extend_ttl`/`bump` in this crate | Error-rate trigger that arms automatic rollback; fallback `DEFAULT_ROLLBACK_ERROR_BPS` (500). |
| `RolloutKey::Flag(Symbol)` (`shared::rollout`) | `bool` | instance | unset — no `extend_ttl`/`bump` in this crate | Per-flag enabled bit, written by `set_feature_flag`, read by `is_feature_enabled`, force-cleared on rollback. |
| `RolloutKey::FlagIndex` (`shared::rollout`) | `Vec<Symbol>` | instance | unset — no `extend_ttl`/`bump` in this crate | Index of every flag ever set, so rollback can disable them all. |

Additionally, `src/dispute_helpers.rs` defines an ad-hoc, non-`DataKey` persistent key `(Symbol::new(env, "disp_st"), dispute_id: u64) -> u32` (`1` = Open) in `DisputeHelper::raise_dispute` / `resolve_dispute`. That module is **not declared** in `lib.rs` (only `errors` and `types` are), so it is never compiled — its key cannot appear in this contract's live storage. `src/escrow_client.rs` is likewise not declared in `lib.rs` and is not compiled; it is a centralised wrapper of `escrow_open_dispute` / `escrow_refund_client` / `escrow_release_payment` / `escrow_get_status` (the last returning a hard-coded `0u32` placeholder) that `lib.rs` duplicates inline instead. Note the symbol divergence: `lib.rs` calls `rel_pay` (as the test mock defines it), while `escrow_client.rs` would call `release_p`.

**TTL constants.**

This crate declares **no TTL constants of its own** and never calls `extend_ttl`/`bump`. The only ledger-denominated values in its ledger are the constructor parameter `auto_resolve_ledgers` and constants inherited from `shared` (listed below, none of which is a storage TTL):

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `auto_resolve_ledgers` | runtime parameter, no default | unknown — the value is whatever the deployer passes to `initialize`; no crate-level default exists | Delay added to `opened_ledger` to form `DisputeRecord.auto_resolve_ledger`; gates `auto_resolve`. |
| `shared::health::DEFAULT_STALL_LEDGERS` | 17,280 | ~1 day (source comment: "~1 day at 5s/ledger") | Inactivity window before a stall anomaly; surfaced in `get_sla_targets` and default `AlertConfig.stall_ledgers`. Not a storage TTL. |
| `shared::health::SLA_HEALTH_CHECK_MAX_LEDGERS` | 60 | ~5 min (source comment: "~5 min at 5s/ledger") | Published poll cadence for off-chain monitors; surfaced by `get_sla_targets`. Not a storage TTL. |
| `shared::health::DEFAULT_ALERT_COOLDOWN_LEDGERS` | 60 | ~5 min | Minimum gap between consecutive `hlth_alrt` events; default `AlertConfig.alert_cooldown_ledgers`. Not a storage TTL. |

**Compile status.** `compiles` — `cargo build -p dispute_arbiter` finishes cleanly (no warnings emitted).


### ecosystem_funding

> unknown — `contracts/ecosystem_funding/src/lib.rs` has no crate-level `//!` module doc; it opens with plain `//` header comments ("Implements Ecosystem Funding Programs — closes #617", followed by the five acceptance criteria). The summary below is reconstructed from that header plus the source.

Source: `contracts/ecosystem_funding/`

**Storage layout.**

Every key is persistent — there is no `env.storage().instance()` or `.temporary()` call anywhere in the crate, including for the admin address and the ID counter.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | persistent | none | The platform admin; written by `initialize`, read by `require_manager_or_admin` as the fallback authority when the caller is not the programme's manager |
| `DataKey::ProgramCount` | `u64` | persistent | none | Programme-ID counter, pre-incremented in `next_id`; also the count of programmes ever created, since it is never decremented |
| `DataKey::Program(u64)` | `FundingProgram` | persistent | none | `{id, name, description, program_type, manager, recipient, total_allocation, total_disbursed, token, status, created_at, completed_at}` |
| `DataKey::Milestones(u64)` | `Vec<ProgramMilestone>` | persistent | none | The whole milestone list for a programme, written whole on each submit/review; each is `{index, title, description, disbursement_amount, status, submitted_at, approved_at, deliverable_uri}` |
| `DataKey::OutcomeCount(u64)` | `u64` | persistent | none | Per-programme count of recorded outcomes; indexes the `Outcome` space and bounds `get_outcomes` |
| `DataKey::Outcome(u64, u64)` | `ProgramOutcome` | persistent | none | `{program_id, recipient, metric_name, metric_value, description, ledger}`, keyed by programme and zero-based outcome index |

**TTL constants.**

None. The crate declares no `const` of any kind, no `extend_ttl` and no `bump`, so the six persistent entries above have no on-chain TTL extension and depend entirely on the contract's own storage-expiration configuration. There is also no ledger-based deadline in the model at all: programme progression is driven by `ProgramStatus` and `ProgramMilestoneStatus` transitions plus `created_at`/`completed_at`/`submitted_at`/`approved_at` stamps, not by elapsed ledgers.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none | — | — | No TTL constant in the crate and no `extend_ttl`/`bump` call at any storage site |

**Compile status.** `compiles` — `cargo check -p ecosystem_funding` and `cargo test -p ecosystem_funding --no-run` both succeed with no warnings. There are no tests to run: the crate has no `#[cfg(test)]` module and no `src/test.rs`, so the test target is empty.


### escrow

> Handles locking, releasing, refunding, and dispute escrow workflows for StellarAid.

Source: `contracts/escrow/`

**Storage layout.**

No `temporary` storage is used anywhere in the crate. USDC balances are **not** in this contract's storage — they live in the token contract; this contract only ever moves them via `token::Client::transfer`.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `PauseKey::Paused` | `bool` | `instance` (`env.storage().instance()`, lib.rs:52/110/153/171/181) | host-managed instance TTL; no `extend_ttl` call in source | Global pause switch. Read by `require_not_paused`; gates `create_escrow` and `refund_client` only. |
| `PauseKey::Admin` | `Address` | `instance` (lib.rs:106/109/147/165/196/225/242) | host-managed instance TTL; no `extend_ttl` in source | Active escrow admin. Must equal the `admin` argument on `pause`/`unpause`/`transfer_admin`/`cancel_admin_transfer`, and supplies the previous-admin side of the rotation event. |
| `PauseKey::PendingAdmin` | `Address` | `instance` (lib.rs:204/217/250/256) | host-managed instance TTL; no `extend_ttl` in source | Successor proposed by `transfer_admin`, consumed by `accept_admin` (removed on accept or on `cancel_admin_transfer`). |
| `DataKey::Escrow(commission_id: Bytes)` | `EscrowRecord` | `persistent` (storage.rs:46/49/52) | `extend_ttl(key, 432_000, 432_000)` on create (lib.rs:298) and on every partial release (lib.rs:538); `extend_ttl(key, get_dispute_ttl_ledgers(env), …)` on `open_dispute` (lib.rs:425) | The custody record: `commission_id`, `client`, `artist`, `amount`, `fee_bps`, `status`, `created_ledger`, `released_amount`. Read/written on every state transition. |
| `DataKey::ReentrancyLock` | `bool` (presence-flag) | `instance` (storage.rs:79/84/89) | host-managed instance TTL; no `extend_ttl` in source | Re-entrancy guard. Set before and removed after every `with_reentrancy_guard` body, including on the `Err` path. |
| `DataKey::DisputeTtlLedgers` | `u32` | `instance` (storage.rs:121/127) | host-managed instance TTL; no `extend_ttl` in source | Configurable dispute-period TTL, read by `open_dispute` to extend a disputed record. Falls back to `DEFAULT_DISPUTE_TTL_LEDGERS` when unset. |
| `DataKey::AtomicCommit(commission_id: Bytes)` | `AtomicCommitMarker` | `persistent` (storage.rs:59/64/69) | **unknown** — no `extend_ttl` call exists for this key anywhere in the crate; it relies on the network's minimum persistent-entry TTL, which the source never states | Cross-transaction commit marker: `state` (`InProgress`/`Settled`/`RolledBack`/`Failed`), `participants`, `confirmed`, `created_ledger`, `settled_ledger`. |
| `RolloutKey::*` — `Phase`, `CanaryBps`, `Canary`, `Stable`, `RollbackErrorBps`, `Flag(Symbol)`, `FlagIndex` | `RolloutPhase` / `u32` / `Address` / `Symbol` / `Vec<Symbol>` | `instance` (written by `shared::rollout`, not by this crate) | host-managed instance TTL | Canary/flag/rollback state, reached only through the pass-through `pub fn`s `set_canary_deployment`, `set_feature_flag`, `set_rollback_trigger`, `trigger_rollback`. |
| `HealthKey::Metrics` | `HealthMetrics` | `instance` (written by `shared::health`) | host-managed instance TTL | ok/error counters and last-activity ledgers, fed by `report_ok` / `report_error` and read by `health_check` / `get_health_metrics`. |
| `HealthKey::AlertConfig` | `AlertConfig` | `instance` (`shared::health`) | host-managed instance TTL | Alert thresholds, set by `set_alert_config`. |
| `HealthKey::LastAlertLedger` | `u32` | `instance` (`shared::health`) | host-managed instance TTL | Alert de-duplication ledger. |
| `crate::pause::PauseDataKey::Paused` | `bool` | `instance` (`shared::pause`, written via `shared::rollout::trigger_rollback`) | host-managed instance TTL | A **second, distinct** pause key from `PauseKey::Paused`. `trigger_rollback` sets it, but nothing in the escrow crate ever reads it — `require_not_paused` reads only `PauseKey::Paused`. |
| USDC token balance of `env.current_contract_address()` | token-contract entry (not this crate's storage) | external (token contract) | unknown — governed by the token contract | The escrowed funds. Balance is read in tests via `token::Client::balance`; the crate itself never calls `balance()` on any code path. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `ESCROW_TTL_LEDGERS` (lib.rs:64) | `432_000` | ≈30 days (source comment: "~30 days at 6s/ledger") | `DataKey::Escrow` — passed as both threshold and extend-to in `create_escrow` (lib.rs:298) and `partial_release` (lib.rs:538) |
| `DEFAULT_DISPUTE_TTL_LEDGERS` (storage.rs:115) | `864_000` | ≈60 days (source comment: "~60 days at 6s/ledger. Roughly twice the base escrow TTL") | Fallback value of `DataKey::DisputeTtlLedgers`; used by `open_dispute` to extend a disputed `DataKey::Escrow` (lib.rs:425) |
| Lower bound enforced by `set_dispute_ttl_ledgers` (lib.rs:451) | `432_000` (= `ESCROW_TTL_LEDGERS`) | ≈30 days | Minimum configurable dispute TTL; `0` or anything below `ESCROW_TTL_LEDGERS` returns `InvalidAmount` |
| `ATOMIC_COMMIT_PARTICIPANTS` (cross_contract.rs:154) | `2` | n/a (not a TTL) | Sides that must confirm before funds may move in the atomic escrow→commission flow |

No `DataKey::AtomicCommit` entry is ever TTL-extended, and no instance key is extended.

**Compile status.** Does not compile. `cargo build -p escrow` reports exactly two errors, both parse-level, both in `contracts/escrow/src/lib.rs`:  1. `error: mismatched closing delimiter: }` at `lib.rs:650:33` — the `env.events().publish(` at `lib.rs:650:9` is unclosed; the `}` at `lib.rs:669` is flagged mismatched, with rustc pointing at the `storage::with_reentrancy_guard(&env, || {` opened at `lib.rs:637` as the delimiter it was "possibly meant for". 2. `error: this file contains an unclosed delimiter` at `lib.rs:845:24` (`mod correlation_tests;`) — the unclosed delimiters are `impl EscrowContract {` (lib.rs:99), the `pub fn auto_release_on_deadline(...) -> Result<(), EscrowError> {` signature (ending lib.rs:567), and the closure `storage::with_reentrancy_guard(&env, || {` (lib.rs:593); the `}` at `lib.rs:825` absorbs all three with mismatched indentation.  Root cause: the file is spliced at `lib.rs:598–599`. `auto_release_on_deadline`'s body stops after `r.status = CommissionStatus::Released;` with no `save_escrow`, no TTL extension, no transfers, no closing `})` or `}`, and the doc comment + `pub fn cancel_escrow(...)` header of the following function (lib.rs:599–613) begin inside that unclosed closure. The region `lib.rs:637–669` mixes the two functions: it opens a second `with_reentrancy_guard` closure, uses `auto_release_ledger` and `remaining` (locals of `auto_release_on_deadline`) inside the `autorls` event, and also references `client_refund`/`client` (from `cancel_escrow`), which cannot be in scope at that nesting.  Consequently unreachable / uncompilable as written: `auto_release_on_deadline` (truncated — its `save_escrow`, TTL call, artist and platform-wallet transfers, completed `autorls` event and closers are absent from the file and were not reconstructed), `cancel_escrow` (header nested in a closure body, body spliced), the 24 `pub fn`s from `get_escrow` (lib.rs:672) to `trigger_rollback` (lib.rs:821), and all 8 `#[cfg(test)]` module declarations (lib.rs:828–845). The 19 `pub fn`s from `initialize` (lib.rs:104) through `partial_release` (lib.rs:484) are the only lexically valid part of the contract block. The failure is parse-level, so no name resolution, macro expansion of `#[contractimpl]`, or type checking runs — `errors.rs`, `storage.rs` and `cross_contract.rs` are individually well-formed and are documented from source as written, but the crate as a whole produces no WASM. No fix was attempted.


### licensing

> This crate has no `//!` module doc comment. Nearest header comment (contracts/licensing/src/lib.rs:1-9): "Implements Creative Licensing Marketplace — closes #616. Acceptance Criteria: Support sub-licensing rights; Implement usage rights marketplace; Add usage tracking and auditing; Support license derivatives; Implement dispute resolution for usage."

Source: `contracts/licensing/`

**Storage layout.**

Every site in this crate uses `env.storage().persistent()`; there is no `instance()` or `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the file.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | The dispute-resolver admin. Read in `expire_license` (alongside the owner) and in `resolve_dispute` (must equal the passed `admin`). Set in `initialize`; never rotated. |
| `DataKey::LicenseCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic license-id counter, initialised to `0`; `next_license_id` (internal) returns `count + 1` and persists it. |
| `DataKey::DisputeCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic dispute-id counter, initialised to `0`; `next_dispute_id` (internal) returns `count + 1` and persists it. |
| `DataKey::License(id)` | `LicenseRecord` | persistent | unset — no `extend_ttl`/`bump` in the crate | `id, asset_id, owner, licensee, license_type, price, sub_licensable, parent_id, status, created_at, expires_at`. Written on create, sub-license, revoke, expire, raise-dispute, and resolve-dispute. `parent_id` is `0` for primary licenses. |
| `DataKey::UsageCount(license_id)` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Per-license usage counter, initialised to `0` at license/sub-license creation. Read with `unwrap_or(0)`; incremented in `record_usage`; used as the bound by `get_usage_entries`. |
| `DataKey::Usage(license_id, index)` | `UsageEntry` | persistent | unset — no `extend_ttl`/`bump` in the crate | Append-only audit log entry: `license_id, user, context, ledger`, keyed by per-license monotonically increasing `index`. |
| `DataKey::Dispute(dispute_id)` | `LicenseDispute` | persistent | unset — no `extend_ttl`/`bump` in the crate | `dispute_id, license_id, complainant, description, outcome, raised_at`. Created `Pending` by `raise_dispute`; `outcome` overwritten by `resolve_dispute`. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none | — | — | This crate declares no ledger/TTL constants and never calls `extend_ttl`/`bump`; every entry is written with the host default TTL. The only ledger value in the crate is the caller-supplied `expires_at` parameter on `create_license` / `create_sub_license` (`0` means no expiry), which is data rather than a constant. |

**Compile status.** `compiles` — `cargo build -p licensing` finishes cleanly (no warnings emitted).


### mentorship

> This crate has no `//!` module doc comment. Nearest header comment (contracts/mentorship/src/lib.rs:1-9): "Implements Mentorship Program Contract — closes #613. Acceptance Criteria: Support mentor-mentee pairing; Track mentorship milestones; Implement feedback collection; Add certification upon completion; Support compensation for mentors."

Source: `contracts/mentorship/`

**Storage layout.**

Every site in this crate uses `env.storage().persistent()`; there is no `instance()` or `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the file.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | Programme admin. Read in `issue_certificate`, where the caller may be the admin, the mentor, or the mentee. Set in `initialize`; never rotated. |
| `DataKey::EngagementCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic engagement-id counter, initialised to `0`; `next_id` (internal) returns `count + 1` and persists it. |
| `DataKey::Engagement(id)` | `MentoringEngagement` | persistent | unset — no `extend_ttl`/`bump` in the crate | `id, mentor, mentee, description, compensation, token, status, created_at, completed_at, certificate_issued`. Written on propose, accept, cancel, auto-complete, and certificate issuance. |
| `DataKey::Milestones(engagement_id)` | `Vec<MentoringMilestone>` | persistent | unset — no `extend_ttl`/`bump` in the crate | The whole milestone list as a single value, rewritten in place on each submit/review. Each entry is `{ index, title, description, status, approved_at }`; `approved_at` is the current ledger on approval and `0` on rejection. |
| `DataKey::FeedbackCount(engagement_id)` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Per-engagement feedback counter. Read with `unwrap_or(0)`; incremented in `submit_feedback`; used as the bound by `get_feedback`. |
| `DataKey::Feedback(engagement_id, index)` | `FeedbackEntry` | persistent | unset — no `extend_ttl`/`bump` in the crate | `engagement_id, author, rating, comment, ledger`, keyed by per-engagement monotonically increasing `index`. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none | — | — | This crate declares no ledger/TTL constants and never calls `extend_ttl`/`bump`; every entry is written with the host default TTL. The only ledger values stored are per-record timestamps (`created_at`, `completed_at`, `approved_at`, `ledger`) and the `milestone_index: u32` key component, none of which is a TTL. |

**Compile status.** `compiles` — `cargo build -p mentorship` finishes cleanly (no warnings emitted).


### messaging

> Messaging contract — artist-client messaging with encryption metadata support. Closes #596.

Source: `contracts/messaging/`

**Storage layout.**

Seven `DataKey` variants across two storage classes. The crate contains **no `extend_ttl` call at all** (verified: zero matches across `src/*.rs`), so every persistent entry relies on the network's default persistent-entry TTL and nothing is explicitly retained or renewed here. Note that `LastSendLedger` — rate-limit state, not message content — is written to **instance** storage, not persistent storage.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Initialized` | `bool` (always `true`) | instance | none — no `extend_ttl` in crate | One-shot guard read by `initialize`; distinct from an `Admin` check |
| `DataKey::Admin` | `Address` | instance | none | Admin set at `initialize`; not consulted by any messaging entry point |
| `DataKey::Conversation(Bytes)` | `Conversation` | persistent | none — no `extend_ttl` in crate | `{conv_id, participant_a, participant_b, message_count, created_ledger}`; `message_count` is the sequence allocator, re-written on every `send_message` |
| `DataKey::Message(Bytes, u32)` | `Message` | persistent | none — no `extend_ttl` in crate | One message per `(conv_id, seq)`; `seq` is 1-based and monotonic per conversation |
| `DataKey::ReadReceipt(Bytes, Address)` | `ReadReceipt` | persistent | none — no `extend_ttl` in crate | Last-read watermark per `(conv_id, reader)`; overwrite-only, so `up_to_seq` can move **backwards** |
| `DataKey::TypingIndicator(Bytes, Address)` | `TypingIndicator` | persistent | none — no `extend_ttl` in crate | `{conv_id, typer, set_ledger}`; expiry is enforced **in application logic** by `get_typing`, not by ledger TTL — the entry is never removed |
| `DataKey::LastSendLedger(Bytes, Address)` | `u32` (ledger sequence) | **instance** | none — no `extend_ttl` in crate | Per-`(conv_id, sender)` rate-limit anchor. Kept in instance storage, so the set of these keys grows with distinct (conversation, sender) pairs and is bounded only by instance-entry limits |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | Reached via `report_ok` / `report_error` / `get_health_metrics` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Reached via `set_alert_config` / `get_alert_config` |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | `hlth_alrt` cooldown anchor; written by `health_check` |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Mirrored into `HealthMetrics.paused`; set to `true` by `trigger_rollback` |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | none | Reached via `set_feature_flag` / `is_feature_enabled` |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates flags so rollback can zero them |

**TTL constants.**

No TTL constant is used for storage retention in this crate — there is no `extend_ttl` site. The ledger figures below are window/length bounds, not retention.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `RATE_LIMIT_LEDGERS` | 12 | ~1 minute (source comment: "At ~5 s/ledger: 12 ledgers ≈ 1 minute") | Minimum ledgers between two sends by the same sender in the same conversation. **Enforced by `send_message` only** — the check is `current_ledger < last_send.saturating_add(12)` ⇒ `RateLimitExceeded`; it is scoped per `(conv_id, sender)`, so it does not rate-limit a sender across conversations |
| `TYPING_EXPIRY_LEDGERS` | 30 | ~2.5 min (source comment: "~30 ledgers (~2.5 minutes at 5 s/ledger)") | Function-local `const` inside `get_typing` (`lib.rs:364`), not a crate constant. Expiry is a read-time filter (`set_ledger + 30 >= current_ledger`, otherwise `None`); the stored entry is never removed and no `set_typing` bound exists |
| `MAX_HISTORY` | n/a — not a ledger bound | n/a | Page-size cap = 100. Enforced by `get_messages` via `limit.min(MAX_HISTORY)`. Despite its doc comment ("Maximum number of messages kept per conversation before the oldest is pruned") **nothing is ever pruned** — it only clamps the requested page |
| `MAX_MESSAGE_LEN` | n/a — byte length | n/a | 4,096 bytes. Enforced by `send_message` ⇒ `MessageTooLong` |
| `MAX_CONV_ID_LEN` | n/a — byte length | n/a | 64 bytes. Enforced by `create_conversation` ⇒ `ConvIdTooLong` |
| `MAX_MSG_ID_LEN` | n/a — byte length | n/a | 64 bytes, declared in `types.rs:12` and referenced by **no** call site. `MessagingError::MsgIdTooLong` is therefore never returned — dead constant, dead error path (both surface as `dead_code`/unreachable) |

**Compile status.** `compiles` — `cargo check -p messaging --lib` succeeds. Two warnings, both dead code: `errors::get_suggestion` and `types::MAX_MSG_ID_LEN` are never used.


### multi_sig

> Multi-Signature Authorization Contract — closes #709, with the signature-validation rationale and the fail-closed posture spelled out at length in the module doc.

Source: `contracts/multi_sig/`

**Storage layout.**

Five `DataKey` variants. This is the only crate of the four that actually calls `extend_ttl`: proposals and collected signatures are bumped to `MULTISIG_TTL_LEDGERS` on **every** write, including the re-save inside `finalize` and the save on the expiry path.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Sole address allowed to rewrite the signer set. Its presence is the `initialize` guard; single-shot, no rotation |
| `DataKey::Config` | `MultiSigConfig` | instance | none | `{signers: Vec<Address>, threshold, expiry_ledgers}`. Replaced wholesale by `configure`, so stored threshold and stored signers can never disagree. Read by nearly every entry point |
| `DataKey::ProposalCount` | `u32` | instance | none | Running count of proposals ever opened, defaulting to `0`. Incremented with a plain `+ 1` in `open_proposal` (no `checked_add`, so it would panic on overflow rather than return a typed error) |
| `DataKey::Proposal(Bytes)` | `Proposal` | persistent | `MULTISIG_TTL_LEDGERS` (1,296,000) via `extend_ttl(key, 1_296_000, 1_296_000)` in `save_proposal` | `{id, action, creator, status, signature_count, created_ledger, expires_ledger}`. Written by `open_proposal`, `approve`, and both `finalize` paths |
| `DataKey::Approval(Bytes, Address)` | `u32` (ledger the signature landed on) | persistent | `MULTISIG_TTL_LEDGERS` (1,296,000) via `extend_ttl(key, 1_296_000, 1_296_000)` in `approve` | One collected signature per `(proposal, signer)`. **Never removed** — a signature cannot be withdrawn, which is what makes "N of M" mean N distinct signers |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | none | Reached via `report_ok` / `report_error` / `get_health_metrics` |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | none | Reached via `set_alert_config` / `get_alert_config` |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | none | `hlth_alrt` cooldown anchor; written by `health_check` |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | none | Mirrored into `HealthMetrics.paused`; set to `true` by `trigger_rollback` |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | none | Reached via the rollout block |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | none | Reached via `set_feature_flag` / `is_feature_enabled` |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates flags so rollback can zero them |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `MULTISIG_TTL_LEDGERS` | 1,296,000 | ~90 days (source comment: "~90 days at 6 s/ledger, matching `contracts/analytics`"; the same value is 75 days at the 5 s rate) | Private crate-level `const` at `lib.rs:79`. Retention window and `extend_ttl` amount for both persistent keys: `DataKey::Proposal` (via `save_proposal`) and `DataKey::Approval` (in `approve`). Passed as both the `threshold` and the `extend_to` argument, so a key is bumped by at most 1,296,000 from the current ledger |
| `MAX_EXPIRY_LEDGERS` | 120,960 | ~7 days (source comment: "~7 days at 5 s/ledger") | Longest signature lifetime an admin may configure. **Enforced by `configure`** (via `validate_config`): `expiry_ledgers == 0 \|\| expiry_ledgers > MAX_EXPIRY_LEDGERS` ⇒ `InvalidExpiry`. Also re-checked in `open_proposal`, where `created_ledger.checked_add(expiry_ledgers)` overflow ⇒ `InvalidExpiry` |
| `DEFAULT_EXPIRY_LEDGERS` | 17,280 | ~24 h (source comment: "~24 h at 5 s/ledger") | Declared in `types.rs:13` as a *suggested* lifetime but **referenced by no call site** — `lib.rs` does not import it and nothing defaults to it. Callers pass `expiry_ledgers` explicitly |
| `MAX_SIGNERS` | n/a — a count | n/a | 10. Enforced by `configure` via `validate_config`: an empty set or `len() > MAX_SIGNERS` ⇒ `InvalidSigners` |
| `MIN_THRESHOLD` | n/a — a count | n/a | 2. Enforced by `configure` via `validate_config`: `threshold < MIN_THRESHOLD \|\| threshold > signers.len()` ⇒ `InvalidThreshold`, so a 1-of-N scheme is rejected at configuration time |

**Compile status.** `compiles` — `cargo check -p multi_sig --lib` succeeds. The `#[cfg(test)] mod test` in `src/test.rs` (29 tests, including the error-code stability pin) compiles against the `rlib` target.


### nft

> This crate has no `//!` module doc comment. Nearest header comment (contracts/nft/src/lib.rs:1-9): "Implements NFT Ownership Certificates — closes #614. Acceptance Criteria: Support NFT minting for completed commissions; Implement royalty tracking; Add secondary sale support; Track NFT ownership history; Implement NFT burn/transfer restrictions."

Source: `contracts/nft/`

**Storage layout.**

Every site in this crate uses `env.storage().persistent()`; there is no `instance()` or `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the file.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Nft(id)` | `NftCertificate` | persistent | unset — no `extend_ttl`/`bump` in the crate | The certificate record: `id, title, metadata_uri, creator, owner, royalty_bps, commission_id, transferable, burnable, status, minted_at`. Written by `mint`, by `transfer` (owner field), `burn` (status), `freeze` (transferable), `update_royalty` (royalty_bps); read by `get_nft` and the internal `load_nft`. |
| `DataKey::NftCount` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Monotonic id counter, seeded to `0` by `initialize`. The internal `next_id` reads it, stores `count + 1`, and returns that value as the new `nft_id`. |
| `DataKey::TransferCount(nft_id)` | `u64` | persistent | unset — no `extend_ttl`/`bump` in the crate | Number of provenance records held for a certificate. Seeded to `0` by `mint`, incremented on every successful `transfer`; read by `get_transfer_count` and used as the upper bound in `get_transfer_history`. |
| `DataKey::Transfer(nft_id, index)` | `TransferRecord` | persistent | unset — no `extend_ttl`/`bump` in the crate | One provenance entry: `nft_id, from, to, price, ledger`. Appended at index `TransferCount(nft_id)` by `transfer`; read by `get_transfer_history`. |
| `DataKey::Admin` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | The privileged admin, set once by `initialize`. Read only by `freeze`, which asserts the caller equals it. There is no admin rotation, no admin removal, and no pause function. |
| `DataKey::RoyaltyToken` | `Address` | persistent | unset — no `extend_ttl`/`bump` in the crate | SEP-41 token contract used to settle royalties. Set by `initialize`; read by `transfer` through `expect("contract not initialized")` and wrapped in a `token::Client` for the royalty payment. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none declared | unknown — no TTL constant is declared anywhere in this crate | unknown — no ledger value exists to convert | unknown — no storage key is ever passed to `extend_ttl` or `bump` |

The crate declares no ledger-denominated constant at all. The `3000` (30% max royalty) and `10_000` figures are basis-point divisors, not TTLs. Consequently no certificate or provenance record is ever TTL-bumped: each persistent entry lives until it lapses under the network's own `min_persistent_entry_ttl` / archival rules, which are not configured anywhere in this repository (neither `.soroban/config.toml` nor `config/testnet_contracts.json` sets them).

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p nft` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The crate builds a `cdylib` + `rlib`, so `NftOwnership` and all nine entry points above are reachable. Two behavioural caveats that compile cleanly but are worth flagging: the royalty on a secondary sale is debited from the seller `from` rather than the buyer `to` (the code and its comment disagree), and `initialize` is not guarded against re-invocation even though it is the only writer of the id counter.


### platform_config

> "Platform Configuration Contract — Protocol-wide parameters, fee governance, and admin authority delegation. Architecture Decision: [ADR-0005](../../docs/ADRs/0005-platform-fee-and-revenue-distribution.md)" (`//!` doc comment, contracts/platform_config/src/lib.rs:1-4)

Source: `contracts/platform_config/`

**Storage layout.**

All of this crate's own keys are `instance` except `Volume(Address)`, which is `persistent`. There is no `temporary()` access anywhere. The `cfgcache` entry is a bare `symbol_short!("cfgcache")` key — it is *not* a `DataKey` variant.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | n/a — instance entry, never bumped in source | The protocol admin. Written by `initialize` and by `accept_admin`; read by every setter's `require_auth()` and surfaced through `get_config().admin`. Also the value read by escrow/dispute_arbiter's `get_adm` path in consumers that supply their own config. `has()` on this key is also the `is_initialized` flag. |
| `DataKey::FeeBps` | `u32` | instance | n/a — instance entry, never bumped in source | The flat platform fee in basis points, capped at `1000` (10%) by `initialize`, `set_fee_bps`, and the `max_fee_bps` check in `set_token_metadata`. Base input to `resolve_effective_fee_bps` and `compute_fees`. |
| `DataKey::PlatformWallet` | `Address` | instance | n/a — instance entry, never bumped in source | Where the platform's share of a fee is paid; the value escrow reads as `get_pw`. Set by `initialize` / `set_platform_wallet`, surfaced by `get_config()`. |
| `DataKey::UsdcToken` | `Address` | instance | n/a — instance entry, never bumped in source | The canonical settlement token; the value escrow and dispute_arbiter read as `get_usdc`. Set by `initialize`, surfaced by `get_config()`. |
| `DataKey::PendingAdmin` | `Option<Address>` | instance | n/a — instance entry, never bumped in source | The proposed admin of a two-step handover. Written by `transfer_admin`, consumed and overwritten with the new `Admin` by `accept_admin`; `get_pending_admin` returns `None` when absent and `accept_admin` maps that to `NoPendingAdmin`. Never cleared otherwise. |
| `DataKey::TokenName` | `soroban_sdk::String` | instance | n/a — instance entry, never bumped in source | Fee-token display name, written only as part of `set_token_metadata` and read through `get_token_metadata`. |
| `DataKey::TokenSymbol` | `soroban_sdk::String` | instance | n/a — instance entry, never bumped in source | Fee-token ticker, written only by `set_token_metadata`, read through `get_token_metadata`. |
| `DataKey::TokenDecimal` | `u32` | instance | n/a — instance entry, never bumped in source | Fee-token decimals, written only by `set_token_metadata`, read through `get_token_metadata`. Not used by any computation. |
| `DataKey::MinFeeBps` | `u32` | instance | n/a — instance entry, never bumped in source | Lower clamp applied by `resolve_effective_fee_bps` / `compute_fees`; `unwrap_or(0)` on read. Set only via `set_token_metadata`. |
| `DataKey::MaxFeeBps` | `u32` | instance | n/a — instance entry, never bumped in source | Upper clamp applied by `resolve_effective_fee_bps` / `compute_fees`; `unwrap_or(1000)` on read. `set_token_metadata` rejects `max_fee_bps > 1000` with `InvalidFeeBps`. Set only via `set_token_metadata`. |
| `DataKey::ActiveEnvironment` | `AddressEnvironment` | instance | n/a — instance entry, never bumped in source | Which namespace `resolve_for_environment` uses; `unwrap_or(AddressEnvironment::Production)` on read. Set by `set_environment`, read by `get_environment` and `resolve_for_environment`. |
| `DataKey::RegistryEntry(AddressEnvironment, Symbol)` | `Address` | instance | n/a — instance entry, never bumped in source | The registry itself: one address per `(Production / Test, name)` pair. Written/overwritten by `register_address`, removed by `unregister_address` (which errors `AddressNotRegistered` on a miss rather than no-opping), read by `get_registered_address`, `registry_entry`, and `resolve_address`. Names are arbitrary `Symbol`s with no allow-list; the two in-repo examples are `escrow` and `usdc`. |
| `DataKey::ResolutionCache(AddressEnvironment, Symbol)` | `ResolutionCacheEntry` | instance | n/a — instance entry; freshness enforced by ledger comparison, not by a TTL bump | Memoised resolution: `{ address, resolved_ledger }`. Populated by `resolve_address` on a registry miss, served while `now - resolved_ledger <= RESOLUTION_CACHE_TTL_LEDGERS`, and explicitly removed by `register_address` / `unregister_address` so a re-registration is never masked. Read by `resolution_cache`. |
| `DataKey::FeeTiers` | `Vec<FeeTier>` | instance | n/a — instance entry, never bumped in source | Volume-tier ladder, kept sorted ascending by `min_volume` by `upsert_fee_tier`; the largest tier whose threshold the payer's volume meets supplies `fee_bps`. Read by `get_fee_tiers`, `resolve_effective_fee_bps`, `compute_fees`. |
| `DataKey::Promotion` | `Option<Promotion>` | instance | n/a — instance entry, never bumped in source | A `{ start_ledger, end_ledger, fee_bps }` window that overrides base and tier fees while `start <= now <= end`. Set by `set_promotion`, removed by `clear_promotion`, read by `is_promotion_active`, `resolve_effective_fee_bps`, `compute_fees`. |
| `DataKey::ReferralConfig` | `Option<ReferralConfig>` | instance | n/a — instance entry, never bumped in source | `{ bps }` — the share of the platform fee diverted to a referrer; may be up to `10_000` (100% of the fee). Set by `set_referral_config`, read by `get_referral_config` and by `compute_fees` (only when a `Some(referrer)` is passed). |
| `DataKey::Volume(Address)` | `i128` | persistent | `extend_ttl(key, VOLUME_TTL_LEDGERS, VOLUME_TTL_LEDGERS)` on every write | Cumulative per-payer volume feeding the tier ladder. Written by `record_volume` with a saturating add; read by `get_volume`. The only persistent key and the only TTL-managed key in the crate. |
| `cfgcache` (`symbol_short!`, not a `DataKey` variant) | `PlatformConfig` | instance | n/a — instance entry, never bumped in source | Cached assembly of `{ admin, fee_bps, platform_wallet, usdc_token }` so repeated cross-contract `get_config` calls cost one read instead of four (closes #648). Written by `get_config` on a miss, dropped by the private `invalidate_config_cache` from every setter that changes one of its four fields (`set_fee_bps`, `set_platform_wallet`, `accept_admin` — `initialize` writes all four but provably cannot race a populated cache, since `get_config` unwraps `Admin` and so panics before `initialize` has run). `set_token_metadata` correctly does not invalidate: `MinFeeBps` / `MaxFeeBps` are not part of the cached struct. The cache is never TTL-bumped and is dropped by `get_config` on any change to its four fields only. |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | n/a — instance entry | `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`. Written by `report_ok` / `report_error`; read by `get_health_metrics`, `health_check`, `detect_anomaly`, `should_rollback`. |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | n/a — instance entry | Degraded/unhealthy bps thresholds, stall window, alert cooldown, alerting on/off. Written by `set_alert_config` (panics `"invalid alert config"` on inverted or out-of-range values); read by `get_alert_config`. |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | n/a — instance entry | Cooldown stamp for the `hlth_alrt` event; written only by `maybe_emit_alert` inside `health_check`. |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | n/a — instance entry | `Off` / `Canary` / `Full` / `RolledBack`; derived from `canary_bps` on every canary write, forced to `RolledBack` by a rollback. Gates `route_to_canary` and `is_feature_enabled`. |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | n/a — instance entry | Traffic share sent to the canary (≤ `10_000`); zeroed by a rollback. |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | n/a — instance entry | The canary contract id registered by `set_canary_deployment`. |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | n/a — instance entry | The stable contract id registered by `set_canary_deployment`. |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | n/a — instance entry | Error rate that arms automatic rollback; defaults to `DEFAULT_ROLLBACK_ERROR_BPS` (500). Set by `set_rollback_trigger` (panics `"invalid rollback trigger"` on `0` or `> 10_000`). |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | n/a — instance entry | One named feature flag. Written by `set_feature_flag`; read by `is_feature_enabled`, which always returns `false` while the phase is `RolledBack`. A rollback forces every indexed flag to `false`. |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | n/a — instance entry | The list of flags a rollback must disable; appended by `set_feature_flag` on first write of a name. |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | n/a — instance entry | Set to `true` only by `trigger_rollback` (via `shared::rollout::trigger_rollback`) and read by `shared::health`'s `is_paused` so a rolled-back contract reports `Unhealthy`. This contract exposes **no** `pause`, `unpause`, or `schedule_recovery` entry point, so nothing here can ever clear the flag. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `VOLUME_TTL_LEDGERS` (`storage.rs:34`) | 432,000 | ~30 days — derived from the source comment "~30 days at 6s/ledger" (comment figure used in preference to the 5 s ledger-close derivation) | The `persistent` key `DataKey::Volume(Address)`. Passed as both `threshold` and `extend_to` in `record_volume`'s `extend_ttl(&key, VOLUME_TTL_LEDGERS, VOLUME_TTL_LEDGERS)`, so each recorded payment renews retention a further 432,000 ledgers and clears the "below threshold" test. |
| `RESOLUTION_CACHE_TTL_LEDGERS` (`storage.rs:6`) | 86,400 | ~5 days — derived from the source comment "~5 days at 5s/ledger" | `DataKey::ResolutionCache(...)` freshness. **Not** a storage TTL: no `extend_ttl` call uses it. It is compared in `resolve_address` as `now - cached.resolved_ledger <= RESOLUTION_CACHE_TTL_LEDGERS`; past that window the registry is re-read and the stamp rewritten. |

No other ledger-denominated constant exists in this crate. (`shared::health` and `shared::rollout` constants such as `DEFAULT_STALL_LEDGERS`, `DEFAULT_ALERT_COOLDOWN_LEDGERS`, and `SLA_HEALTH_CHECK_MAX_LEDGERS` are thresholds and poll intervals, not storage TTLs, and none of them is ever passed to `extend_ttl`.) Every `instance` key in this table is therefore unmanaged: no entry is ever TTL-bumped or archived on a schedule, so liveness depends entirely on the network's minimum entry TTL.

**Compile status.** `fails to compile`  > **Compile status:** verified with `cargo check -p platform_config`, which fails with exactly one diagnostic: `error: this file contains an unclosed delimiter` — pointing at `contracts/platform_config/src/lib.rs:492:12` (`mod tests;`), reporting `27 | impl PlatformConfigContract {` as the unclosed delimiter and `260 | ) -> Result<ResolutionCacheEntry, ConfigError> {` as the one "that might not be properly closed". The `pub fn resolution_cache` at lib.rs:256-261 ends on `.ok_or(ConfigError::AddressNotRegistered)` with no closing brace, so its body is never terminated. > > **Consequences.** (1) Parsing aborts at the crate root, so **no artifact is produced at all** — neither `cdylib` nor `rlib`, hence no `PlatformConfigContractClient` and no deployable WASM. Every entry point in the table above is unavailable in practice, including the ones that parse correctly. (2) Within the source text, everything from lib.rs:262 to the end of the impl block at line 488 is swallowed into `resolution_cache`'s unterminated body and is therefore never a member of the `#[contractimpl]` block: the entire #690 fee surface (`upsert_fee_tier`, `remove_fee_tier`, `get_fee_tiers`, `set_promotion`, `clear_promotion`, `set_referral_config`, `get_referral_config`, `record_volume`, `get_volume`, `resolve_effective_fee_bps`, `compute_fees`, `is_promotion_active`) and the entire health/rollout surface (`health_check`, `get_health_metrics`, `get_sla_targets`, `set_alert_config`, `get_alert_config`, `detect_anomaly`, `report_ok`, `report_error`, `set_feature_flag`, `is_feature_enabled`, `set_canary_deployment`, `route_to_canary`, `get_rollout_state`, `set_rollback_trigger`, `should_rollback`, `trigger_rollback`), together with their events. (3) Three sibling files have the same defect and are masked because the parser never reaches them: `storage.rs:159-162` (`pub fn remove_resolution_cache` is unterminated, so `get_fee_tiers`, `set_fee_tiers`, `upsert_fee_tier`, `remove_fee_tier`, `get_promotion`, `set_promotion`, `clear_promotion`, `get_referral_config`, `set_referral_config`, `get_volume`, and `record_volume` after it are all textually inside that function); `types.rs:44-46` (`pub struct ResolutionCacheEntry` is unterminated, so `FeeTier`, `Promotion`, `ReferralConfig`, and `FeeBreakdown` are inside it); and `tests.rs:275-283` (`fn registry_entry_returns_full_record` is unterminated). Each of the four files is short exactly one closing brace, so all four breakages must be fixed before the crate will build. (4) `fees.rs` and `errors.rs` are brace-balanced and would compile on their own. Independently of the parse error, `ConfigError` has a duplicate discriminant (`AddressNotRegistered = 6` and `InvalidTier = 6`), which the `#[contracterror]` expansion turns into an unreachable match arm, and lib.rs:16-19 imports `FeeTokenMetadata` and `PlatformConfig` twice from the same `types` module (legal, since both paths resolve to the same item, but redundant).


### rate_limiter

> Rate Limiter Smart Contract — implements configurable rate limiting for account activities to prevent abuse; this module can be used across other contracts to limit specific actions. Closes #710.

Source: `contracts/rate_limiter/`

**Storage layout.**

Three `DataKey` variants across two storage classes. This is the only crate of the four that gives a persistent record a **dynamic** TTL: the extension is `window_ledgers - elapsed` rather than a fixed constant, so a record always expires exactly when its own window rolls over. This crate exposes **no** `shared` health/rollout surface, so no `shared` keys are reachable from it.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Written once by `initialize`; its presence is the single-shot guard. Not rotatable. It is never *read* for comparison — `set_limit` and `reset_limits` authorize the `admin` **parameter** without checking it against this stored value |
| `DataKey::RateLimitConfig` | `Map<RateLimitKey, RateLimitConfig>` | instance | none | The whole limit table in one entry, read-modify-written by `initialize` (via the internal `initialize_default_config`) and by `set_limit`. `load_config` falls back to an empty `Map` when the entry is absent, so an unconfigured key yields `NotInitialized` from `check_rate_limit` rather than a storage error |
| `DataKey::RateLimitRecord(RateLimitKey, Address)` | `RateLimitRecord` | persistent | **dynamic** — `extend_ttl(record_key, ttl_ledgers, ttl_ledgers)` where `ttl_ledgers = config.window_ledgers.saturating_sub(elapsed)` and `elapsed = current_ledger.saturating_sub(record.first_ledger)`; called in `check_rate_limit` on every hit | `{key, account, count, first_ledger, last_ledger}` — the per-`(action class, account)` window state. The only key `reset_limits` removes, across all three classes. `saturating_sub` on both subtractions is deliberate: a record holding a *future* ledger (after a ledger-number regression following a restore, or a test moving the ledger backwards) would underflow and panic in a debug build, and saturating to zero keeps the record inside its current window — the fail-closed direction |
| `shared::*` | — | — | — | None — this contract has no health/rollout entry points, so no `shared` key is reachable through its public interface |

**TTL constants.**

The default window is a literal; every other window is whatever an admin writes into the config map. There is no maximum-length bound on `window_ledgers` and no `extend_ttl` other than the per-record one above.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `DEFAULT_WINDOW_LEDGERS` | 28,800 | ~24 hours (source comment: "~24 hours at 5s/ledger") | Private crate-level `const` at `lib.rs:40`. The `window_ledgers` written for **all three** default limit classes by `initialize` (via the internal `initialize_default_config`): `CommissionsPerArtist`, `DisputesPerUser`, and `EscrowsPerUser`. Not a cap — `set_limit` may set any non-zero `window_ledgers`, and the default is never re-applied afterwards |
| `DEFAULT_COMMISSION_LIMIT` | n/a — a count | n/a | 5. Default `limit` for `RateLimitKey::CommissionsPerArtist` ("Max 5 commissions per artist per day" per the source comment). Overridable by `set_limit` |
| `DEFAULT_DISPUTE_LIMIT` | n/a — a count | n/a | 3. Default `limit` for `RateLimitKey::DisputesPerUser` ("Max 3 disputes per user per day"). Overridable by `set_limit` |
| `DEFAULT_ESCROW_LIMIT` | n/a — a count | n/a | 10. Default `limit` for `RateLimitKey::EscrowsPerUser` ("Max 10 escrows per user per day"). Overridable by `set_limit` |

The three limit counts are the **only** values exposed on-chain, via the `rl_init` event payload. Nothing reads them back — `get_limit` reads the config map, not the constants.

**Compile status.** `compiles` — `cargo check -p rate_limiter --lib` succeeds, with 4 warnings: three dead-code items spliced in from the `include!`d `contracts/semver_types.rs` (`CURRENT_STORAGE_SCHEMA`, `min_compatible_for`, `is_compatible` — all unused because this crate skips `impl_semver_queries!()`), plus one unused-variable warning in `check_rate_limit`. The 20 unit tests in `src/test.rs` include an error-code stability pin.


### recruitment

> This crate has no `//!` module doc comment. It also has no top-of-file `//` header comment: `src/lib.rs` opens with `#![no_std]`. Nearest `//` comment in the file is the section header at contracts/recruitment/src/lib.rs:480: "── Health monitoring (#678) and gradual rollout (#684) ──". `Cargo.toml` likewise carries no description comment (only the `[package.metadata.stellar-aid]` block).

Source: `contracts/recruitment/`

**Storage layout.**

Two `instance` keys, the rest `persistent`; there is no `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the crate.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | n/a — instance entry, never bumped in source | Written by `initialize` and then read **only** as the initialisation flag (`has_admin`). It is never compared against any caller and never required to sign: the recruitment flow authorises the employer and the applicant, not an admin, so there is no administrative override, no operator kill switch, and no way to rotate or read back the admin (no getter is exposed). |
| `DataKey::MaxApplicants` | `u32` | instance | n/a — instance entry, never bumped in source | Per-posting cap on applicants, set by `initialize` (which rejects `0`). Read in `apply_for_job` with `unwrap_or(0)`; a missing key therefore blocks all applications. Only settable at initialisation. |
| `DataKey::Job(Bytes)` | `Job` | persistent | unset — no `extend_ttl`/`bump` in the crate | `{ id, employer, title, budget, openings, filled, applicant_count, status, posted_ledger }`. Written by `post_job`, `close_job`, `accept_offer` (the `filled` / `status` update), and `apply_for_job` (the `applicant_count` increment). Read by `get_job` and by the internal `load_job`. |
| `DataKey::Applicants(Bytes)` | `Vec<Address>` | persistent | unset — no `extend_ttl`/`bump` in the crate | The applicant list for a job, appended by `apply_for_job` in arrival order; read by `get_applicants`. Entries are never removed on withdraw/reject, so it is a complete submission log, not a live shortlist. |
| `DataKey::Pipeline(Bytes)` | `Pipeline` | persistent | unset — no `extend_ttl`/`bump` in the crate | Live headcount per funnel stage. The internal `shift_pipeline` decrements the source bucket (saturating) and increments the destination on every stage change, writing the whole struct; `load_pipeline` falls back to `Pipeline::default()`. |
| `DataKey::Application(Bytes, Address)` | `Application` | persistent | unset — no `extend_ttl`/`bump` in the crate | `{ job_id, applicant, proposal_uri, rate, stage, applied_ledger, updated_ledger }`. Written by `apply_for_job` and by the internal `move_stage` on each transition; read by `get_application` and the internal `load_application`. |
| `DataKey::Offer(Bytes, Address)` | `Offer` | persistent | unset — no `extend_ttl`/`bump` in the crate | `{ rate, start_ledger, made_ledger }`, written by `make_offer` and read by `get_offer`. Deliberately kept beside the application so a declined offer stays auditable; it is never deleted, and a re-offer overwrites it while the application stage is not already `Offered`. |
| `DataKey::Performance(Bytes, Address)` | `Performance` | persistent | unset — no `extend_ttl`/`bump` in the crate | `{ reviews, total_rating, last_rating, last_ledger, last_note }`, accumulated by `record_performance` (reviews and total_rating increment, last_* overwritten). Read by `get_performance` and `get_average_rating`. |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | n/a — instance entry | `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`; written by `report_ok` / `report_error`, read by `get_health_metrics`, `health_check`, `detect_anomaly`, `should_rollback`. |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | n/a — instance entry | Alerting thresholds; written by `set_alert_config` (panics `"invalid alert config"` on inverted/out-of-range values), read by `get_alert_config`. |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | n/a — instance entry | Cooldown stamp for `hlth_alrt`; written only by `maybe_emit_alert` inside `health_check`. |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | n/a — instance entry | `Off` / `Canary` / `Full` / `RolledBack`; gates `route_to_canary` and `is_feature_enabled`. |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | n/a — instance entry | Canary traffic share (≤ `10_000`); zeroed by a rollback. |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | n/a — instance entry | Canary contract id from `set_canary_deployment`. |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | n/a — instance entry | Stable contract id from `set_canary_deployment`. |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | n/a — instance entry | Error rate arming automatic rollback; defaults to `DEFAULT_ROLLBACK_ERROR_BPS` (500). |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | n/a — instance entry | One named feature flag, set by `set_feature_flag`, read by `is_feature_enabled`. |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | n/a — instance entry | Flags a rollback must disable; appended by `set_feature_flag`. |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | n/a — instance entry | Set to `true` only by `trigger_rollback`, and read only by `shared::health` to report `Unhealthy`. This contract exposes no `pause`/`unpause` entry point, and none of the recruitment entry points consults this flag, so a rollback has no functional effect on hiring — it can never be cleared here either. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none declared | unknown — no TTL constant is declared in this crate's own source | unknown — no ledger value exists to convert | unknown — no storage key is ever passed to `extend_ttl` or `bump` |

The crate declares no ledger-denominated constant. `Stage::rank()` values (`0`…`4`) and the `1..=5` rating range are ordering and validation bounds, not TTLs. Every persistent key above is therefore unmanaged: a job, its applications, its pipeline counters, its offer, and its reviews all live until they lapse under the network's own `min_persistent_entry_ttl` / archival rules, which are not configured anywhere in this repository — so an open job, or a running pipeline, is not protected by any renewal logic. The `shared::health` / `shared::rollout` constants reachable through the re-exported surface (`DEFAULT_STALL_LEDGERS`, `DEFAULT_ALERT_COOLDOWN_LEDGERS`, `SLA_HEALTH_CHECK_MAX_LEDGERS`) are thresholds and poll intervals, and none is passed to `extend_ttl`.

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p recruitment` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The `#[cfg(test)] mod test` module (contracts/recruitment/src/test.rs, 371 lines) is brace- and paren-balanced and compiles under `cargo test`. Behavioural notes that do not affect compilation: the `admin` written by `initialize` is never used for authorization, so the `admin`-taking health/rollback setters are gated on whoever signs rather than on the configured admin; and the `admin`-parameterised `set_alert_config` / `report_ok` / `report_error` / `set_feature_flag` / `set_canary_deployment` / `set_rollback_trigger` / `trigger_rollback` are reachable by any address that signs.


### reputation

> Review Moderation & Appeal System — review submission with rating and comment, review reporting, admin moderation queue and decision tracking, an appeal mechanism for artists and clients, and escalation to the dispute arbiter. Closes #604.

Source: `contracts/reputation/`

**Storage layout.**

Both fragments declare a `#[contracttype] enum DataKey` in `types.rs` — the first at lines 103–118 (unterminated) and the second at lines 159–169. Variant names collide across the two, so which set a caller observes is **unknown — the crate does not parse; only one `DataKey` can survive a fix**. Fragment A = the moderation contract, Fragment B = the scoring contract.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` (A, line 104) | `Address` | instance | none | Moderation admin; written once by `initialize`, read by `require_admin` |
| `DataKey::Review(Bytes)` (A, line 105) | `ReviewRecord` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `submit_review` | A review keyed by caller-chosen `review_id`; status is **overwritten in place** by `report_review`, `moderate_review`, and `resolve_appeal` |
| `DataKey::Report(Bytes, u32)` (A, line 107) | `ReportRecord` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `report_review` | One report per `(review_id, report_index)`; scanned linearly for the per-reporter dedup check |
| `DataKey::ReportCount(Bytes)` (A, line 109) | `u32` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `report_review` | Number of reports for a review, also the next append index |
| `DataKey::Appeal(Bytes)` (A, line 111) | `AppealRecord` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `file_appeal` | At most one appeal per review; status mutated by `resolve_appeal` and `escalate_appeal` |
| `DataKey::ModerationEntry(Bytes, u32)` (A, line 113) | `ModerationRecord` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `moderate_review` | Append-only moderation decision history per review |
| `DataKey::ModerationCount(Bytes)` (A, line 115) | `u32` | persistent | `REPUTATION_TTL_LEDGERS` (1,296,000) via `extend_ttl` in `moderate_review` | Number of moderation entries, also the next append index |
| `DataKey::QueueEntry(u32)` (A, line 117) | `Bytes` (review_id) | instance | none | Admin moderation queue slot, appended in `report_review` |
| `DataKey::QueueSize` (A, line 118) | `u32` | instance | none | Next queue slot / queue length. Grows monotonically and is never decremented — `moderate_review` appends to the moderation history but, per its own doc, is supposed to remove the entry from the queue, and no such removal is written |
| `DataKey::Admin` (B, line 161) | `Address` | instance | none | Scoring admin; `initialize` in fragment B writes it **without** `require_auth()` |
| `DataKey::Moderator(Address)` (B, line 162) | `bool` (always `true`; `remove_moderator` uses `remove`) | instance | none | Moderator role set, additive over the admin |
| `DataKey::ReviewsForArtist(Address)` (B, line 164) | `Vec<Review>` | persistent | **none — no `extend_ttl` anywhere in fragment B** | Every review for an artist in submission order; loaded whole and rewritten on each change, so cost grows linearly with review count |
| `DataKey::HasReviewed(Address, Address)` (B, line 166) | `bool` (always `true`) | persistent | none | Dedup guard, keyed `(artist, client)`; never cleared |
| `DataKey::ReputationScore(Address)` (B, line 168) | `u32` | persistent | none | Cached 0–100 score, recomputed and rewritten on every review state change |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `REPUTATION_TTL_LEDGERS` | 1,296,000 | ~90 days (source comment: "~90 days at 6 s/ledger"; the same value is 75 days at the 5 s rate) | Fragment A only: `Review`, `Report`, `ReportCount`, `Appeal`, `ModerationEntry`, `ModerationCount`. Fragment B's `ReviewsForArtist`, `HasReviewed`, and `ReputationScore` get **no** TTL extension |

Fragment B declares two non-TTL limits: `MAX_COMMENT_LEN = 512` bytes (enforced by `require_comment_len` → `CommentTooLong`, applied to the review comment, the dispute reason, and the moderation note) and `MIN_REVIEWS_FOR_FULL_CONFIDENCE = 5`.

**Compile status.** `does not compile` — unclosed delimiter. `contracts/reputation/src/lib.rs` contains **two** `#[contract]` types (`ReputationContract` at line 1 and `Reputation`, spliced in at line 344) whose `DataKey`, `ReviewStatus` and `ReputationError` definitions collide, and both declare an `initialize` entry point. Only one of the two can survive a fix, so which storage keys and error codes a deployed `reputation` would expose is **unknown — the crate does not parse, so no semantic analysis is possible**. Both `src/tests.rs` (targeting `ReputationContractClient`, `rating_x10` 10–50) and `src/test.rs` (targeting `ReputationClient`, `rating` 1–5, `get_reputation`) are wired up, confirming the duplication. `ReputationError` also has duplicate names and duplicate discriminants (5, 6, 7, 8) within a single fragment, so its final variant set and code assignment are likewise undetermined.


### revenue_sharing

> This crate has no `//!` module doc comment. It also has no top-of-file `//` header comment: `src/lib.rs` opens with `#![no_std]`. Nearest `//` comment in the file is the section header at contracts/revenue_sharing/src/lib.rs:326: "── Health monitoring (#678) and gradual rollout (#684) ──". `Cargo.toml` likewise carries no description comment. The crate-level context for this design lives in docs/ADRs/0005-platform-fee-and-revenue-distribution.md.

Source: `contracts/revenue_sharing/`

**Storage layout.**

Two `instance` keys, the rest `persistent`; there is no `temporary()` access, and no `extend_ttl`/`bump` call anywhere in the crate.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | n/a — instance entry, never bumped in source | Written by `initialize` and then read **only** as the initialisation flag (`has_admin`). It is never compared against a caller and never required to sign — every governance action is authorised by the per-agreement `owner` — so there is no protocol-level admin override, no way to rotate it, and no getter. |
| `DataKey::HistoryLimit` | `u32` | instance | n/a — instance entry, never bumped in source | Cap on retained `RevenueEntry` records per agreement, set by `initialize` (which rejects `0`) and read by the internal `push_history` with `unwrap_or(0)`. Only settable at initialisation. Because the read defaults to `0`, a missing key makes `push_history`'s `while history.len() >= limit` loop non-terminating; the value is never extended or lowered afterwards. |
| `DataKey::Agreement(Bytes)` | `Agreement` | persistent | unset — no `extend_ttl`/`bump` in the crate | `{ id, owner, token, status, terms_version, total_revenue, total_distributed, entry_count, created_ledger, updated_ledger }`. Written by `create_agreement`, `update_splits` (`terms_version` / `updated_ledger`), `set_status` (`status` / `updated_ledger`), and `record_revenue` (both totals, `entry_count`, `updated_ledger`). Read by `get_agreement`, `get_report`, and the internal `load_agreement`. `total_revenue` and `total_distributed` always move together, so they are equal by construction. |
| `DataKey::Splits(Bytes)` | `Vec<Participant>` | persistent | unset — no `extend_ttl`/`bump` in the crate | The current `(account, share_bps)` terms, written by `create_agreement` and wholly replaced by `update_splits`; read by `get_splits` and the internal `load_splits`. |
| `DataKey::Earnings(Bytes, Address)` | `i128` | persistent | unset — no `extend_ttl`/`bump` in the crate | Lifetime amount attributed to one account under one agreement, accumulated by the internal `add_earnings` inside `record_revenue`; read by `get_earnings` with a `0` default. It is a single running total — a changed split does not restate past earnings. |
| `DataKey::History(Bytes)` | `Vec<RevenueEntry>` | persistent | unset — no `extend_ttl`/`bump` in the crate | Ring buffer of `{ sequence, source, gross, distributed, terms_version, ledger, memo }`, appended by the internal `push_history` from `record_revenue` and trimmed from the front to `HistoryLimit`; read by `get_history`. `entry_count` on the agreement keeps the true lifetime sequence number that the trimming discards. |
| `shared::health::HealthKey::Metrics` | `HealthMetrics` | instance | n/a — instance entry | `{ ok_count, error_count, last_ok_ledger, last_error_ledger, paused }`; written by `report_ok` / `report_error`, read by `get_health_metrics`, `health_check`, `detect_anomaly`, `should_rollback`. |
| `shared::health::HealthKey::AlertConfig` | `AlertConfig` | instance | n/a — instance entry | Alerting thresholds; written by `set_alert_config` (panics `"invalid alert config"` on inverted/out-of-range values), read by `get_alert_config`. |
| `shared::health::HealthKey::LastAlertLedger` | `u32` | instance | n/a — instance entry | Cooldown stamp for `hlth_alrt`; written only by `maybe_emit_alert` inside `health_check`. |
| `shared::rollout::RolloutKey::Phase` | `RolloutPhase` | instance | n/a — instance entry | `Off` / `Canary` / `Full` / `RolledBack`; gates `route_to_canary` and `is_feature_enabled`. |
| `shared::rollout::RolloutKey::CanaryBps` | `u32` | instance | n/a — instance entry | Canary traffic share (≤ `10_000`); zeroed by a rollback. |
| `shared::rollout::RolloutKey::Canary` | `Address` | instance | n/a — instance entry | Canary contract id from `set_canary_deployment`. |
| `shared::rollout::RolloutKey::Stable` | `Address` | instance | n/a — instance entry | Stable contract id from `set_canary_deployment`. |
| `shared::rollout::RolloutKey::RollbackErrorBps` | `u32` | instance | n/a — instance entry | Error rate arming automatic rollback; defaults to `DEFAULT_ROLLBACK_ERROR_BPS` (500). |
| `shared::rollout::RolloutKey::Flag(Symbol)` | `bool` | instance | n/a — instance entry | One named feature flag, set by `set_feature_flag`, read by `is_feature_enabled`. |
| `shared::rollout::RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | n/a — instance entry | Flags a rollback must disable; appended by `set_feature_flag`. |
| `shared::pause::PauseDataKey::Paused` | `bool` | instance | n/a — instance entry | Set to `true` only by `trigger_rollback` and read only by `shared::health` to report `Unhealthy`. **This crate does not call `shared::pause` at all** — despite `contracts/shared/src/pause.rs:10` naming `revenue_sharing` as one of its five consumers — so no entry point here reads or clears the flag, and `record_revenue` ignores it. `shared::pause::DEFAULT_RECOVERY_DELAY_LEDGERS` (17,280) and `MAX_RECOVERY_DELAY_LEDGERS` (120,960) exist in `shared` but are unreachable from this contract, since no `schedule_recovery` / `unpause` entry point is exposed. |

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none declared | unknown — no TTL constant is declared in this crate's own source | unknown — no ledger value exists to convert | unknown — no storage key is ever passed to `extend_ttl` or `bump` |

`TOTAL_BPS` (`10_000`) and `MAX_PARTICIPANTS` (`20`) in types.rs are split-validation bounds, not TTLs. Every persistent key above is therefore unmanaged: an agreement, its splits, every participant's running earnings, and its history buffer live until they lapse under the network's own `min_persistent_entry_ttl` / archival rules, which are not configured anywhere in this repository. There is in particular no retention mechanism keeping the `Earnings` accumulator or `History` alive for a long-running agreement.

**Compile status.** `compiles`  > **Compile status:** verified by `cargo check -p revenue_sharing` (offline) — `Finished \`dev\` profile`, no errors and no warnings emitted for this crate. The `#[cfg(test)] mod test` module (contracts/revenue_sharing/src/test.rs, 300 lines) is brace- and paren-balanced and compiles under `cargo test`. Behavioural notes that do not affect compilation: the `admin` written by `initialize` is never used for authorization, so the `admin`-taking health/rollback setters are gated on whoever signs rather than on the configured admin; and the crate does not call `shared::pause` at all, contradicting `contracts/shared/src/pause.rs:10` which names `revenue_sharing` as one of the five `pause`/`unpause` consumers — so a `trigger_rollback` pause flag is set here but can neither be honoured nor cleared.


### search

> Search contract — indexes artists for discovery with filtering, sorting, pagination, keyword ("full-text") metadata, and search analytics (#599).

Source: `contracts/search/`

**Storage layout.**

Four `DataKey` variants across two storage classes. Like `messaging`, this crate contains **no `extend_ttl` call at all** (verified: zero matches), so `DataKey::Listing` — the only persistent entry, and the one that would need restoring if a listing expired — is never renewed and relies entirely on the network's default persistent-entry TTL. It exposes no `shared` health/rollout surface, so no `shared` key is reachable from it.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | none | Written once by `initialize`; its presence is both the `initialize` guard (`has_admin`) and the "is the contract initialized" test used by `index_artist` and `search`. Not rotatable. Unlike every other admin-gated entry point, `set_rating` and the two listing toggles compare the caller against **this stored value** |
| `DataKey::Listing(Address)` | `ArtistListing` | persistent | none — **no `extend_ttl` in crate**, so the listing is never explicitly retained | `{artist, skills, price, rating, keywords, indexed_ledger, active}`. `rating` is written only by `set_rating` and preserved across re-indexing; `indexed_ledger` and `active` are overwritten by `index_artist` and the toggles. `ListingNotFound` when absent |
| `DataKey::AllArtists` | `Vec<Address>` | instance | none | Every artist ever indexed, in indexing order. Appended once per new artist (guarded by an `is_new` check) and **never pruned**, so a deactivated artist is still scanned on every query. `search` iterates this whole list |
| `DataKey::Analytics` | `SearchAnalytics` | instance | none | `{total_searches: u64, total_indexed: u32}`. Read through `unwrap_or_default()`; `total_indexed` is incremented only for a first-time index (re-indexing does not double-count), `total_searches` on every `search` |
| `shared::*` | — | — | — | None — this contract has no health/rollout entry points, so no `shared` key is reachable through its public interface |

**TTL constants.**

No TTL constant and no `extend_ttl` call exist in this crate. The bounds below are input-size caps enforced by the entry points named in the last column.

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `MAX_PAGE_SIZE` | n/a — a result count | n/a | 50. Hard cap on a single page. **Enforced by `search` only**: `page_size == 0 \|\| page_size > MAX_PAGE_SIZE` ⇒ `InvalidPageSize`. Note what it does *not* bound: `search` still walks all of `DataKey::AllArtists`, loading and filtering every listing before paginating, so per-call cost scales with the total number of ever-indexed artists. The tests confirm the bound is real in both directions (`0` and `1000` are both rejected) |
| `MAX_SKILLS` | n/a — a tag count | n/a | 20. **Enforced by `index_artist` only**: `skills.len() > MAX_SKILLS` ⇒ `TooManySkills` |
| `MAX_KEYWORDS` | n/a — a tag count | n/a | 30. **Enforced by `index_artist` only**: `keywords.len() > MAX_KEYWORDS` ⇒ `TooManyKeywords` |

**Compile status.** `compiles` — `cargo check -p search --lib` succeeds with no warnings. The 12 unit tests in `src/test.rs` compile against the `rlib` target.


### subscription

> This crate has no `//!` module doc comment. The nearest comment is the `///` doc on the private `charge` fn (contracts/subscription/src/lib.rs:78-80): "Charge a period against the subscriber's prepaid credit. Renewals are driven off this balance rather than a live transfer so that `renew` can be called by anyone once a period ends, without the subscriber signing each time." The first `//` section header in the file is line 151, `// ── Tiers and benefits ──`.

Source: `contracts/subscription/`

**Storage layout.**

Two classes are used: `env.storage().instance()` for the four configuration keys, `env.storage().persistent()` for every per-tier and per-subscriber record. There is no `temporary()` access anywhere in the crate, and no `extend_ttl`, `bump`, or `extend_instance` call anywhere in `src/` (verified by grep over the whole crate, including `test.rs`).

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | unset — no `extend_ttl`/`bump` in the crate | The privileged admin, written once by `initialize` and never rotated or removed. Read by the private `has_admin` and `require_admin`; `require_admin` does the `Address` read and then `admin.require_auth()`, so every admin entry point is gated on this exact stored address. |
| `DataKey::Token` | `Address` | instance | unset — no `extend_ttl`/`bump` in the crate | SEP-41 token used to settle deposits and withdrawals. Written by `initialize`; read by `deposit` and `withdraw` with `.unwrap()` (a missing key is a host panic, not a typed error — reachable only if the instance were tampered with, since `initialize` always sets it). |
| `DataKey::GraceLedgers` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Admin-supplied grace-window length in ledgers, written by `initialize` with **no upper bound and no zero check**. Read through `get_u32` (which defaults to `0`) by `coverage_end` — extending entitlement coverage for auto-renewing subscriptions — and by `renew`, where `ledger > period_end_ledger + GraceLedgers` → `GraceExpired`. |
| `DataKey::HistoryLimit` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Cap on retained `PaymentRecord` entries per subscriber, written by `initialize` (which rejects `0` with `InvalidAmount`). Read through `get_u32` by `record_payment`, which `pop_front()`s oldest entries while `payments.len() >= limit`. The `0` guard matters: a `0` limit would make that `while` loop spin on an empty vector forever. |
| `DataKey::Tier(tier_id)` | `Tier` | persistent | unset — no `extend_ttl`/`bump` in the crate | The plan definition: `tier_id, name, price, period_ledgers, benefits, active`. Written by `create_tier` (after rejecting a duplicate id, `price <= 0`, and `period_ledgers == 0`) and re-written by `set_tier_active`, which only flips `active`. Read by the private `load_tier` on behalf of `get_tier`, `set_tier_active`, `has_benefit`, `subscribe`, and `renew`. `period_ledgers` is the billing period: `subscribe` sets `period_end_ledger = ledger + tier.period_ledgers` and `renew` advances it by exactly that amount. |
| `DataKey::Subscription(subscriber)` | `Subscription` | persistent | unset — no `extend_ttl`/`bump` in the crate | The member's plan state: `subscriber, tier_id, status, started_ledger, period_end_ledger, renewals, total_paid, auto_renew`. Written by the private `save_subscription` from `subscribe`, `renew`, `cancel`, and `lapse`; read by `load_subscription` on behalf of `has_benefit`, `subscribe`, `renew`, `cancel`, `lapse`, `get_subscription`, `is_active`, and `in_grace`. `subscribe` refuses a second sign-up only while the existing record is still `is_active`; a lapsed one is silently overwritten with `renewals: 0` and `total_paid: tier.price`. |
| `DataKey::Credit(account)` | `i128` | persistent | unset — no `extend_ttl`/`bump` in the crate | Prepaid token balance in the smallest denomination. Credited by `deposit`, debited by `withdraw` and by the private `charge` (called from `subscribe` and `renew`). Read by `credit_of`, which defaults to `0` for an unknown account — so `get_credit` on a never-seen address returns `0` rather than erroring. |
| `DataKey::Payments(subscriber)` | `Vec<PaymentRecord>` | persistent | unset — no `extend_ttl`/`bump` in the crate | Bounded charge history. Written by the private `record_payment` (from `subscribe` and `renew`), which caps length at `HistoryLimit`; read by `get_payments`, defaulting to an empty `Vec`. `sequence` is set to `renewals + 1`, so it is a monotonically increasing counter, not a `Payments` length. |

The 16 delegated health/rollout entry points also touch storage, but through the `shared` crate rather than this crate's `DataKey`: `HealthKey::Metrics`, `HealthKey::AlertConfig`, `HealthKey::LastAlertLedger`, `RolloutKey::Phase`, `RolloutKey::CanaryBps`, `RolloutKey::Canary`, `RolloutKey::Stable`, `RolloutKey::RollbackErrorBps`, `RolloutKey::Flag(Symbol)`, `RolloutKey::FlagIndex`, and `PauseDataKey::Paused` — all read/written with `env.storage().instance()` in `contracts/shared/src/health.rs` and `contracts/shared/src/rollout.rs`, also with no `extend_ttl`.

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none declared | unknown — this crate declares no `const` of any kind in `src/`, and no `extend_ttl`/`bump` call, so there is no literal TTL value to report | unknown — no ledger value exists to convert | unknown — no storage key is ever passed to a TTL-extension call in this crate |
| `DataKey::GraceLedgers` (grace window — runtime value, not a constant) | unknown — admin-supplied `grace_ledgers` argument of `initialize`, with no default in source; `get_u32` falls back to `0`, which would disable the window entirely | ≈ `grace_ledgers × 5 s` (Stellar ledger close ≈ 5 s); no source comment states a duration | Purely a **ledger-number comparison** against `env.ledger().sequence()`: `renew` rejects with `GraceExpired` when `ledger > period_end_ledger + GraceLedgers`, and `coverage_end` extends benefits to `period_end_ledger + GraceLedgers` for auto-renewing subscriptions (`in_grace` reports the same window). **No entry point bumps any TTL** — not `renew`, not `lapse`, not the mutators that rewrite the record. Writing `Subscription(subscriber)` again in `renew`/`cancel`/`lapse` refreshes nothing TTL-wise. |
| `Tier.period_ledgers` (billing period — runtime value, not a constant) | unknown — per-tier `period_ledgers` argument of `create_tier`, which only rejects `0`; there is no default in source | ≈ `period_ledgers × 5 s`; no source comment states a duration | Sets `Subscription.period_end_ledger = started_ledger + period_ledgers` in `subscribe`, and each `renew` adds exactly `period_ledgers` again. Drives `renew` (`ledger <= period_end_ledger` → `RenewalNotDue`), `coverage_end`, `is_active`, `in_grace`, and `PaymentRecord.period_end_ledger`. **No entry point bumps any TTL**; `Subscription`, `Payments`, and `Credit` entries all live on the network's own entry-lifetime defaults. |

For completeness, the delegated `shared::health` module does declare ledger constants, but they classify *activity*, not entry lifetime, and none of them is a TTL: `DEFAULT_STALL_LEDGERS: u32 = 17_280` (source comment: "~1 day at 5s/ledger" — applied by `is_stalled` via `AlertConfig.stall_ledgers`), `DEFAULT_ALERT_COOLDOWN_LEDGERS: u32 = 60` (≈ 5 min; rate-limits successive `hlth_alrt` events in `maybe_emit_alert`), and `SLA_HEALTH_CHECK_MAX_LEDGERS: u32 = 60` (source comment: "~5 min at 5s/ledger"; published in `SlaTargets` as a polling cadence for off-chain monitors). None is passed to `extend_ttl`, and `shared` contains no `extend_ttl` call either, so the "which entry points bump TTL" answer for this contract is: **none**.

**Compile status.** `compiles`  > Verified with `cargo check -p subscription -p verification --offline` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`, with no errors and no warnings for this crate. `crate-type = ["cdylib", "rlib"]`, so `SubscriptionContract` and all 35 entry points are reachable. Behaviours that compile cleanly but deserve flagging in the storage docs: `initialize` is unauthenticated (first caller becomes admin); `GraceLedgers` is unvalidated and `get_u32`'s `0` fallback silently disables the grace window; no entry point extends any TTL, so `Tier`, `Subscription`, `Credit`, and `Payments` all depend on the network's default entry lifetimes; and `withdraw` can drain credit earmarked for a future `renew`.


### verification

> This crate has no `//!` module doc comment. The nearest comment is the `///` doc on the quality weights (contracts/verification/src/lib.rs:18-19): "Weights applied to each quality criterion; they sum to 100 so the blended score stays on the same 0..=100 scale as the individual marks." The first `//` section header in the file is line 94, `// ── Verification badges (#598) ──`.

Source: `contracts/verification/`

**Storage layout.**

Two classes are used: `env.storage().instance()` for the five configuration keys and the reviewer allow-list, `env.storage().persistent()` for every per-artist record. There is no `temporary()` access anywhere in the crate, and no `extend_ttl`, `bump`, or `extend_instance` call anywhere in `src/` (verified by grep over the whole crate, including `test.rs`).

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `DataKey::Admin` | `Address` | instance | unset — no `extend_ttl`/`bump` in the crate | The privileged admin, written once by `initialize`, never rotated or removed. Read by `has_admin`, `get_admin`, and `require_admin` (which does `admin.require_auth()`). The same address is also implicitly a reviewer: `require_reviewer` accepts a caller whose address equals the stored admin even when no `DataKey::Reviewer` entry exists. |
| `DataKey::MinScore` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Minimum blended quality score for approval, in `0..=100`. Written by `initialize` (rejecting `> 100`) and by `set_min_score` (same check). Read by `review_portfolio` (`score >= MinScore` → `Verified`); changing it does not retroactively re-evaluate any existing verdict. |
| `DataKey::MinWorkCount` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Minimum `work_count` an artist must declare. Written by `initialize` only — there is no setter. Read by `submit_portfolio` and `update_portfolio`; a shortfall gives `InvalidWorkCount`. `0` is accepted, which disables the check. |
| `DataKey::UpdateInterval` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Portfolio refresh interval in ledgers. Written by `initialize` and by `set_update_interval` (rejecting `0` with `InvalidInterval`). Read by `review_portfolio` on approval only: `next_update_ledger = ledger + UpdateInterval`. `set_update_interval` does **not** recompute `next_update_ledger` on already-verified portfolios, so a lowered interval has no effect until each portfolio is re-reviewed. |
| `DataKey::HistoryLimit` | `u32` | instance | unset — no `extend_ttl`/`bump` in the crate | Cap on retained `VerificationRecord` entries, written by `initialize` (which rejects `0` with `InvalidInterval`). Read through `get_u32` by `push_history`, which `pop_front()`s oldest entries while `history.len() >= limit`. The `0` rejection matters, because the sibling `push_badge_history` guards with `.max(1)` while `push_history` does not — a `0` limit would make its `while` loop spin forever. |
| `DataKey::Reviewer(reviewer)` | `bool` | instance | unset — no `extend_ttl`/`bump` in the crate | Reviewer allow-list entry, always stored as `true`. Written by `add_reviewer` and **removed** by `remove_reviewer` (the only `remove` call in the crate). Read by `is_reviewer` and, in `require_reviewer`, to decide whether `start_review`, `review_portfolio`, `issue_badge`, and `revoke_badge` are permitted. Removal takes effect immediately and does not cancel an in-flight `UnderReview` claim. |
| `DataKey::Portfolio(artist)` | `Portfolio` | persistent | unset — no `extend_ttl`/`bump` in the crate | The verification record: `artist, metadata_uri, work_count, status, score, revision, submitted_ledger, reviewed_ledger, reviewer, next_update_ledger`. Written by the private `save_portfolio` from `submit_portfolio`, `update_portfolio`, `start_review`, `review_portfolio`, and `flag_update_required`; read by `load_portfolio` on behalf of `update_portfolio`, `start_review`, `review_portfolio`, `flag_update_required`, `get_portfolio`, `is_verified`, and `requires_update`. Created once per artist (`PortfolioExists` on re-submission) — the storage key is the artist's address, so an artist can never hold two portfolios. |
| `DataKey::History(artist)` | `Vec<VerificationRecord>` | persistent | unset — no `extend_ttl`/`bump` in the crate | Bounded review history. Written by the private `push_history` from `update_portfolio` (a `Resubmitted` record) and `review_portfolio` (an `Approved`/`Rejected` record); read by `get_history`, defaulting to an empty `Vec`. Trimming is `pop_front()`-based, so the oldest entries are discarded first. |
| `DataKey::Badge(artist, badge_type)` | `Badge` | persistent | unset — no `extend_ttl`/`bump` in the crate | One badge per (artist, type): `artist, badge_type, issuer, status, issued_ledger, expires_ledger, revoke_reason`. Written by the private `save_badge` from `issue_badge` and `revoke_badge`; read by `load_badge` on behalf of `issue_badge`, `revoke_badge`, `get_badge`, and `is_badge_active`. `expires_ledger` is a **ledger-number** comparison, not a TTL: `issue_badge` sets it to `ledger + valid_for_ledgers`, or to `0` meaning "never expires on its own", and `badge_is_active` treats `expires_ledger == 0` as perpetual. Re-issuing overwrites the whole record, including resetting `issued_ledger`, `issuer`, and clearing `revoke_reason` — so a renewal of a revoked badge silently produces a fresh, unrevoked one. |
| `DataKey::BadgeHistory(artist)` | `Vec<BadgeEvent>` | persistent | unset — no `extend_ttl`/`bump` in the crate | Bounded badge audit trail: `badge_type, action, actor, ledger, note`. Written by the private `push_badge_history` from `issue_badge` (`Issued`/`Renewed`) and `revoke_badge` (`Revoked`); read by `get_badge_history`, defaulting to an empty `Vec`. Capped at `HistoryLimit` with an explicit `.max(1)` floor. |
| `DataKey::BadgeTypes(artist)` | `Vec<BadgeType>` | persistent | unset — no `extend_ttl`/`bump` in the crate | Every badge type ever issued to this artist, active or not. Written by the private `track_badge_type` (append-if-absent, only on `issue_badge`) and read by `get_artist_badge_types`, defaulting to an empty `Vec`. Never pruned, so a revoked badge type stays listed — the doc comment on `get_artist_badge_types` says to consult `is_badge_active`/`get_badge` for current status. |

The 16 delegated health/rollout entry points also touch storage, but through the `shared` crate rather than this crate's `DataKey`: `HealthKey::Metrics`, `HealthKey::AlertConfig`, `HealthKey::LastAlertLedger`, `RolloutKey::Phase`, `RolloutKey::CanaryBps`, `RolloutKey::Canary`, `RolloutKey::Stable`, `RolloutKey::RollbackErrorBps`, `RolloutKey::Flag(Symbol)`, `RolloutKey::FlagIndex`, and `PauseDataKey::Paused` — all read/written with `env.storage().instance()` in `contracts/shared/src/health.rs` and `contracts/shared/src/rollout.rs`, also with no `extend_ttl`.

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| none declared as a TTL | unknown — this crate declares only the four quality-weight `const`s (`WEIGHT_ORIGINALITY` 30, `WEIGHT_TECHNIQUE` 30, `WEIGHT_CONSISTENCY` 20, `WEIGHT_PRESENTATION` 20, summing to the `100` divisor), and no `extend_ttl`/`bump` call, so there is no literal TTL value to report | unknown — no ledger value exists to convert | unknown — no storage key is ever passed to a TTL-extension call in this crate |
| `DataKey::UpdateInterval` (portfolio refresh window — runtime value, not a constant) | unknown — admin-supplied `update_interval` argument of `initialize` / `set_update_interval`, which only rejects `0`; there is no default in source | ≈ `update_interval × 5 s` (Stellar ledger close ≈ 5 s); no source comment states a duration | A **ledger-number deadline** on the stored record: `review_portfolio` sets `next_update_ledger = reviewed_ledger + UpdateInterval` when approving (and `0` when rejecting); the private `is_stale` treats a `Verified` portfolio as stale once `env.ledger().sequence() > next_update_ledger`, and that derived staleness gates `is_verified`, `requires_update`, and `flag_update_required` (which writes `status: UpdateRequired`). **No entry point bumps any TTL** — not `update_portfolio`, not `flag_update_required`, not `review_portfolio`; re-writing `Portfolio(artist)` refreshes nothing TTL-wise. |
| `issue_badge`'s `valid_for_ledgers` (badge expiry window — per-call runtime value, not a constant) | unknown — caller-supplied argument with no default in source | ≈ `valid_for_ledgers × 5 s`; no source comment states a duration | Sets `Badge.expires_ledger = env.ledger().sequence() + valid_for_ledgers`, or `0` when the caller passes `0` (never expires; exercised by the `badge_with_zero_validity_never_expires` test, whose comment notes the advance stays "within the test sandbox's default storage TTL"). Enforced purely as a ledger comparison in `badge_is_active` (`expires_ledger == 0 || sequence <= expires_ledger`) and surfaced by `is_badge_active`. **No entry point bumps any TTL** — `issue_badge` renewal rewrites the record (new `expires_ledger`) but performs no `extend_ttl`, so the *entry's* lifetime is entirely the network default and independent of the *badge's* logical validity. |

So the pairing the storage docs need: the expiry windows are **logical ledger comparisons over persistent entries that are never TTL-bumped**. The only entry points that rewrite the relevant records are `review_portfolio` / `update_portfolio` / `flag_update_required` for the portfolio window and `issue_badge` / `revoke_badge` for the badge window, and every one of them writes state without touching entry lifetime. For completeness, the delegated `shared::health` module does declare ledger constants, but they classify *activity*, not entry lifetime, and none is a TTL: `DEFAULT_STALL_LEDGERS: u32 = 17_280` (source comment: "~1 day at 5s/ledger"; applied by `is_stalled` via `AlertConfig.stall_ledgers`), `DEFAULT_ALERT_COOLDOWN_LEDGERS: u32 = 60` (≈ 5 min; rate-limits successive `hlth_alrt` events), and `SLA_HEALTH_CHECK_MAX_LEDGERS: u32 = 60` (source comment: "~5 min at 5s/ledger"; published in `SlaTargets` as a monitor polling cadence). None is passed to `extend_ttl`, and `shared` contains no `extend_ttl` call either.

**Compile status.** `compiles`  > Verified with `cargo check -p subscription -p verification --offline` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`, with no errors and no warnings for this crate. `crate-type = ["cdylib", "rlib"]`, so `Verification` and all 40 entry points are reachable. Behaviours that compile cleanly but deserve flagging in the storage docs: `initialize` is unauthenticated (first caller becomes admin); no entry point extends any TTL, so `Portfolio`, `History`, `Badge`, `BadgeHistory`, and `BadgeTypes` all live on the network's default entry lifetimes while the refresh and expiry windows are pure ledger comparisons; `submit_portfolio` writes no `History` record; `issue_badge` can grant `PortfolioVerified` with no portfolio at all; and `is_reviewer` disagrees with the real gate in `require_reviewer` for the admin address.


### shared

> State migration and upgrade safety helpers (closes #595). — module doc of `upgrade.rs`; the crate is a 10-module library (`config`, `errors`, `pause`, `types`, `upgrade`, `health`, `rollout`, `correlation`, `validation`, `version`) with no crate-level `//!` doc.

Source: `contracts/shared/`

**Storage layout.**

Every site in the crate uses `env.storage().instance()` — there is not one `persistent()` or `temporary()` call, and the crate never calls `extend_ttl`/`bump`, so instance entries are governed by the contract's own instance TTL configuration. Keys are spread across four `#[contracttype]` key enums; there is no single `DataKey` in this crate.

| Key | Value type | Storage class | TTL | Purpose |
|---|---|---|---|---|
| `PauseDataKey::Paused` | `bool` | instance | none — no `extend_ttl` in crate | Emergency-stop flag; read by `pause::is_paused`, `upgrade::require_paused_for_upgrade`, `rollout::trigger_rollback`, and a private `health::is_paused` |
| `PauseDataKey::RecoveryEta` | `u32` | instance | none | Ledger at which a scheduled recovery matures; `0` is the unambiguous "nothing armed" sentinel |
| `UpgradeKey::Version` | `ContractVersion` (`{major,minor,patch}`) | instance | none | Current on-chain semver; written by `upgrade::record_upgrade` and `version::store`/`seed`, read by `upgrade::get_version` and `version::query` |
| `UpgradeKey::LastUpgradeLedger` | `u32` | instance | none | Ledger of the last `record_upgrade` |
| `HealthKey::Metrics` | `HealthMetrics` | instance | none | `ok_count`/`error_count`, `last_ok_ledger`/`last_error_ledger`, mirrored `paused` flag |
| `HealthKey::AlertConfig` | `AlertConfig` | instance | none | Degraded/unhealthy bps thresholds, stall window, alert cooldown, `alerting_enabled` |
| `HealthKey::LastAlertLedger` | `u32` | instance | none | Cooldown anchor for `hlth_alrt` rate limiting; `0` means never alerted |
| `RolloutKey::Phase` | `RolloutPhase` | instance | none | `Off` / `Canary` / `Full` / `RolledBack` |
| `RolloutKey::CanaryBps` | `u32` | instance | none | Traffic share routed to canary (0–10000 bps) |
| `RolloutKey::Canary` | `Address` | instance | none | Canary contract id |
| `RolloutKey::Stable` | `Address` | instance | none | Stable contract id |
| `RolloutKey::RollbackErrorBps` | `u32` | instance | none | Error rate at which automatic rollback arms |
| `RolloutKey::Flag(Symbol)` | `bool` | instance | none | Named feature flag, one entry per flag |
| `RolloutKey::FlagIndex` | `Vec<Symbol>` | instance | none | Enumerates flags so `disable_all_flags` can zero them on rollback |

`config.rs`, `correlation.rs` and `validation.rs` write no storage at all; `config.rs` performs read-only cross-contract invocations, `correlation.rs` is pure hashing plus one event publish, and `validation.rs` is length checking.

**TTL constants.**

| Constant | Ledgers | Approx. duration | Applies to |
|---|---|---|---|
| `DEFAULT_RECOVERY_DELAY_LEDGERS` | 17,280 | ~24 h (source comment: "~24 h at 5 s/ledger") | Suggested `schedule_recovery` delay; never used as a default inside the crate — the caller passes the delay |
| `MAX_RECOVERY_DELAY_LEDGERS` | 120,960 | ~7 days (source comment: "~7 days at 5 s/ledger") | Hard upper bound a `schedule_recovery` caller may arm; `0` is also rejected |
| `DEFAULT_STALL_LEDGERS` | 17,280 | ~1 day (source comment: "~1 day at 5s/ledger") | Inactivity window before `is_stalled` raises an anomaly; seeds `AlertConfig::stall_ledgers` |
| `DEFAULT_ALERT_COOLDOWN_LEDGERS` | 60 | ~5 min | Minimum gap between successive `hlth_alrt` events; seeds `AlertConfig::alert_cooldown_ledgers` |
| `SLA_HEALTH_CHECK_MAX_LEDGERS` | 60 | ~5 min (source comment: "~5 min at 5s/ledger") | Documented polling cadence for off-chain monitors; surfaced in `SlaTargets::health_check_max_ledgers` |

Not durations, but declared alongside them: `SLA_AVAILABILITY_BPS = 9_990` (99.90%), `SLA_MAX_ERROR_BPS = 10` (0.10%), `DEFAULT_DEGRADED_ERROR_BPS = 100` (1%), `DEFAULT_UNHEALTHY_ERROR_BPS = 500` (5%), `DEFAULT_ROLLBACK_ERROR_BPS = 500` (5%), `CURRENT_STORAGE_SCHEMA = 1`, and the private `BPS_DENOM = 10_000` in both `health.rs` and `rollout.rs`. `validation.rs` declares four byte-length limits rather than ledger windows: `MAX_TITLE_LEN = 128`, `MAX_DESCRIPTION_LEN = 512`, `MAX_MEMO_LEN = 256`, `MAX_ID_LEN = 64`.

**Compile status.** `compiles` — `cargo check -p shared --lib` succeeds. Two documentation/behaviour gaps are present but do not affect compilation: the `upgrade` module doc advertises `require_upgrade_safe` and `export_storage_keys`, neither of which is defined anywhere in the crate (the implemented set is `record_upgrade`, `get_version`, `require_paused_for_upgrade`, `signal_migration_needed`), and the same doc calls the completion event `upgrade_complete` while the code publishes `symbol_short!("upgraded")`.


---

## 9. Supply-chain posture

> Closes #879 and #880. This section is the human-readable record behind
> `deny.toml`, `.cargo/audit.toml`, `make deny` and `make audit`. The two config
> files are the machine-enforced policy; this section is the reasoning and the
> evidence behind the exceptions in them.

### 9.1 What runs, and when

| Command | Tool | Reads | Enforces |
|---|---|---|---|
| `make deny` | `cargo-deny` 0.20.2 | `deny.toml` | advisories, licence allowlist, source provenance, duplicate versions, wildcard requirements |
| `make audit` | `cargo-audit` 0.22.2 | `.cargo/audit.toml` | RustSec advisories, unsound code, crates.io yank status |

Both are pinned-policy rather than best-effort: `deny.toml` sets
`maximum-db-staleness = "P7D"` so a stale advisory database fails the run, and
`.cargo/audit.toml` sets `stale = false` plus `fetch = true` for the same
reason. An audit against a frozen database is not an audit.

The two tools are complementary rather than redundant, and the reason is
worth recording because it is not obvious:

- `cargo deny` resolves a **target- and feature-filtered** graph. It only sees
  crates that would actually be compiled for the targets listed in
  `[graph].targets`.
- `cargo audit` reads **`Cargo.lock` directly**, so it sees every resolved
  package regardless of platform or feature.

That difference is not theoretical. In the first run of this audit, `cargo deny
check` reported five findings and `cargo audit` reported two *more*:

| Advisory | Crate | Version in lock | Reached via | In a contract's wasm graph? |
|---|---|---|---|---|
| RUSTSEC-2024-0344 | `curve25519-dalek` | 4.1.1 | `ed25519-dalek` ← `soroban-env-host` | no — host-side verification |
| RUSTSEC-2026-0097 | `rand` | 0.8.5 | `sdk/`, `soroban-env-host` | no |
| RUSTSEC-2025-0056 | `adler` | 1.0.2 | `miniz_oxide` ← `backtrace` ← `soroban-env-host` | no |
| RUSTSEC-2024-0436 | `paste` | 1.0.15 | `wasmi_core` ← `soroban-wasmi` | no |
| RUSTSEC-2026-0285 | `rustls` | 0.23.43 | `hyper-rustls`, `tokio-rustls` ← `reqwest` (feature-gated) | no — **not resolved at all** |
| RUSTSEC-2026-0009 | `time` | 0.3.44 | `serde_with` (`time_0_3` feature, off) | no — **not resolved at all** |
| RUSTSEC-2023-0031 | `spin` (yanked) | 0.9.8 | `soroban-wasmi` | no |

`rustls` and `time` are the interesting pair. They are present in
`Cargo.lock` and vulnerable there, but *no* currently-resolved feature or
platform combination compiles them: `reqwest` selects `native-tls` on macOS, and
`serde_with`'s `time_0_3` feature is off. They are latent — enabling
`reqwest/rustls-tls` or `serde_with/time_0_3` would pull the vulnerable versions
straight into a build. This is precisely the class of problem a
`Cargo.lock`-level audit catches and a graph-level one does not, and it is why
both commands are wired up.

### 9.2 Initial audit result

Baseline: `upstream/main` at `f6abc91`, RustSec `advisory-db` at `e211151`
(2026-09-25), `Cargo.lock` with 323 packages — 296 from crates.io, 27 path
packages in this workspace, zero git dependencies.

Seven advisories were found. **Four were fixed, two are waived, one is
unavoidable.**

#### Fixed in `Cargo.lock`

All four are patch-level bumps inside existing semver ranges, so no manifest
changed and no behaviour was re-specified.

| Advisory | Crate | Before → after | Why it matters here |
|---|---|---|---|
| RUSTSEC-2024-0344 | `curve25519-dalek` | 4.1.1 → **4.1.3** | Timing variability in `Scalar29::sub` / `Scalar52::sub`. Reachable from `soroban-env-host`'s signature verification, i.e. every SDK-side signature check. |
| RUSTSEC-2026-0097 | `rand` | 0.8.5 → **0.8.8** | Unsound with a custom logger calling `rand::rng()`. `rand` is a **direct dependency of `sdk/`**, which is the crate that signs and submits transactions. |
| RUSTSEC-2026-0285 | `rustls` | 0.23.43 → **0.23.45** | TLS 1.3 handshake messages accepted across encryption-level boundaries. Latent behind `reqwest/rustls-tls`; relevant to any Linux or non-macOS deployment of `sdk/`. |
| RUSTSEC-2026-0009 | `time` | 0.3.44 → **0.3.47** | Denial of service via stack exhaustion. Latent behind `serde_with/time_0_3`; note `serde_with` *is* in the contract wasm graph, so this one is one feature-flag away from a contract build. |

`cargo update -p curve25519-dalek` initially resolved to 5.0.0, which would have
introduced a *second* copy of the crate (`soroban-env-host` accepts both
`^4.1` and `^5.0`). Pinning to 4.1.3 for both edges keeps the graph at one copy
and closes the advisory at the same time.

The `time` bump moved the whole `serde` family to 1.0.229 (`serde`,
`serde_core`, `serde_derive`), because `serde_derive` and `serde` cannot be
mixed across the 1.0.220 split. Verified by building a contract for
`wasm32-unknown-unknown` afterwards.

#### Waived, with a documented reason

| Advisory | Crate | Why it is acceptable |
|---|---|---|
| RUSTSEC-2025-0056 | `adler` 1.0.2 | Unmaintained; upstream's fix is the separate `adler2` crate. We reach `adler` through `miniz_oxide` 0.7 ← `backtrace` 0.3 ← `soroban-env-host`, and `backtrace` 0.3.x pins `miniz_oxide ^0.7`, so **no resolver run can reach `adler2`**. It backs local panic symbolisation in the test VM. |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | Unmaintained; upstream suggests the `pastey` fork. It is a proc-macro dependency of `wasmi_core` ← `soroban-wasmi`, compiled into the test VM. No patch release of `paste` itself exists. |

Neither crate appears in the `wasm32-unknown-unknown`, dev-dependency-free graph
of any of the 24 contracts, and neither is a vulnerability. Both waivers must be
revisited if either ever becomes reachable from a contract.

#### Unavoidable, and why the policy was loosened for it

`spin` 0.9.8 is **yanked** on crates.io. `soroban-env-common` 21.2.1 pins
`soroban-wasmi` 0.31.1-soroban.20.0.1 exactly; that build requires `spin ^0.9`;
and 0.9.8 is both the last 0.9.x release *and* yanked. `cargo update -p
soroban-wasmi` reports "0 packages to latest compatible versions", so this is not
fixable from this repository.

A yank is a statement about *new adoption*, not a published vulnerability, and
`spin` never enters a contract's wasm. `deny.toml` therefore sets
`advisories.yanked = "warn"` rather than `"deny"`, with a comment recording that
this must be flipped back as soon as soroban-sdk ships a wasmi on `spin` 0.10.
Note that `ignore` in `deny.toml` cannot waive this: a yank comes from the
crates.io index, not the advisory database.

#### Current state

```
$ make deny
advisories ok, bans ok, licenses ok, sources ok

$ make audit
warning: 1 allowed warning found
```

Both exit 0. `make deny` emits nine warnings, all documented above or in
§9.3; `make audit` emits the single `spin` yank warning. There are **zero
outstanding vulnerability advisories**.

### 9.3 What is deliberately still a warning

`deny.toml` sets `bans.multiple-versions = "warn"`. Eight crates are present at
two versions each:

| Crate | Versions | Reached via |
|---|---|---|
| `base64` | 0.13.1 / 0.22.1 | `soroban-spec` / `reqwest` → `hyper-util` |
| `core-foundation` | 0.9.4 / 0.10.1 | `system-configuration` / `security-framework` |
| `getrandom` | 0.2.11 / 0.4.3 | `rand_core` / `tempfile` → `native-tls` |
| `miniz_oxide` | 0.7.4 / 0.9.1 | `backtrace` ← `soroban-env-host` / `flate2` ← `sdk` |
| `stellar-strkey` | 0.0.2 / 0.0.8 | workspace dep / `stellar-xdr` |
| `syn` | 2.0.39 / 3.0.4 | `serde_derive` / `tokio`, `thiserror` 2 |
| `thiserror` | 1.0.55 / 2.0.20 | `soroban-sdk` / workspace dep |
| `thiserror-impl` | 1.0.55 / 2.0.20 | `thiserror` 1 / `thiserror` 2 |

Duplicate versions matter for contracts because duplicated code competes for a
fixed page budget. The reason this is a warning rather than an error is
evidence-based: `core-foundation`, `getrandom`, `miniz_oxide`,
`base64` 0.22.1, `syn` 3 and `thiserror` 2 all enter through `reqwest`,
`flate2` or `thiserror` in the off-chain `sdk/` crate, and every one of them is
absent from all 24 contracts' wasm graphs. None inflates a deployed blob. The
remaining pairs (`base64` 0.13.1, `stellar-strkey`, `syn` 2, `thiserror` 1) are
forced by soroban-sdk 21 / stellar-xdr 21 semver requirements. Raise to `"deny"`
as each pair collapses upstream.

### 9.4 Licence and source policy

`[licenses]` is an allowlist of permissive licences only — Apache-2.0 (with and
without the LLVM exception), BSD-2-Clause, BSD-3-Clause, ISC, MIT, Unlicense,
Zlib, 0BSD, Unicode-3.0. The deployed contracts are closed-source platform code,
so any reciprocal or copyleft licence is a licensing decision the project is not
equipped to review, and an allowlist fails closed by default. Licence parsing
runs at `confidence-threshold = 0.93` rather than cargo-deny's 0.8 default, so
an unreviewed licence expression cannot slip through on a low-confidence guess.

`[licenses.private].ignore = true` waives one specific diagnostic: the
workspace's own crates carry no `license` field, so "unpublished crate without a
licence" would otherwise fire for all 27 path crates. As part of making the
policy coherent, `publish = false` was added to all 28 non-published manifests
and an explicit `version` was added to the 27 internal path dependencies — a
path dependency with no version is invisible to cargo-deny's duplicate and
source checks.

`[sources]` denies unknown registries and denies all git dependencies.
Everything must come from crates.io, and a crate cannot appear from a git remote
that does not show up in `Cargo.lock` review.

### 9.5 Reproducing this

```bash
make deny     # cargo deny  check — policy gate
make audit    # cargo audit       — Cargo.lock advisory sweep
```

Both tools are installed on demand. Neither is a substitute for reading a diff:
`[licenses.private].ignore`, the `advisories.ignore` list, and
`bans.multiple-versions` are all places where a well-meaning change can weaken
the gate, which is why each carries a comment explaining what it costs.
