# State Retention and Pruning Policy

> Closes **#651** — add contract state pruning strategy.
>
> Related: [STORAGE.md](./STORAGE.md),
> [ADR-0007](./ADRs/0007-storage-data-model-and-ttl-management.md),
> [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md),
> [OPERATIONAL_RUNBOOK.md](./OPERATIONAL_RUNBOOK.md),
> [EMERGENCY_PROCEDURES.md](./EMERGENCY_PROCEDURES.md).

---

## 1. The rule that overrides everything else

**In a money-handling contract, audit-relevant and dispute-relevant state is
never prunable.**

That means, without exception:

- **Escrows** — `contracts/escrow`: `DataKey::Escrow(Bytes)`,
  `DataKey::AtomicCommit(Bytes)`. An escrow is either holding a client's funds
  or it is the evidence of who is owed what. A "cleaned up" escrow is an
  uncollectable debt.
- **Disputes** — `dispute_arbiter`. A dispute is a live claim. Deleting one
  destroys the basis of an arbitration and may leave funds stranded.
- **Claims and payments** — `donation`, `withdrawal`, `commission_agreement`
  (agreements and milestones), `revenue_sharing`, `creator_fund`,
  `subscription`, `campaign`. A completed payment is a tax and accounting
  record; a pending one is a claim.
- **Reputation and reviews** — `reputation`, `verification`. Not funds, but
  they determine who gets paid next, and they are the record a user disputes
  when their score is wrong.
- **Anything a user could be told "we have no record of" and be right.**

A record in this class is retained until its **ledger entry is archived by the
network itself**, through ordinary TTL expiry. It is never removed by contract
code. This is not a stylistic preference: the same entry can be restored from
the bucket list until it is fully archived, so a contract-side `remove` destroys
a record that the network was still preserving *and* is cheaper for the user
than restoring it.

**Pruning is only ever permitted for derived, reproducible, non-claimable
data.** In this workspace that means analytics history and nothing else.

---

## 2. Two mechanisms, and which to prefer

| Mechanism | What it does | Destructive | Use when |
|-----------|--------------|-------------|----------|
| **TTL management** — `env.storage().persistent().extend_ttl(&key, threshold, extend_to)` | Renews an entry's lifetime. The network archives it when the ledger stops archiving. The data is restorable from the bucket list until then. | No | **Default.** Anything an active user may still care about. |
| **Explicit prune** — `env.storage().persistent().remove(&key)` | Deletes the entry now. | Yes | Derived data that can be recomputed, and only after its retention window has passed. |

TTL is preferred everywhere it is sufficient, and it is what almost every write
path in this workspace already does:

| Constant | Value (ledgers) | Where | On what |
|----------|-----------------|-------|--------|
| `ANALYTICS_TTL_LEDGERS` | 1,296,000 (~90 d) | `analytics` | `Earning`, `EarningCount`, `Metrics` |
| `ESCROW_TTL_LEDGERS` | 432,000 (~30 d) | `escrow` | `DataKey::Escrow`, extended on every write |
| `DEFAULT_DISPUTE_TTL_LEDGERS` | 864,000 (~60 d) | `escrow` | a disputed escrow, replacing the base TTL |
| `VOLUME_TTL_LEDGERS` | 432,000 (~30 d) | `platform_config` | `DataKey::Volume` |
| `RESOLUTION_CACHE_TTL_LEDGERS` | 86,400 (~5 d) | `platform_config` | `DataKey::ResolutionCache` |
| `RATE_LIMIT_LEDGERS` | 12 (~1 min) | `messaging` | per-conversation send rate |
| rate-limit window TTL | per-config | `rate_limiter` | rate-limit records, set to the window end |

**A record's TTL is renewed on write, not on read.** A user who has stopped
touching a record is the one whose record expires — see §5.

---

## 3. Archival criteria

A record is **archivable** only when all of these hold.

| # | Criterion | Why |
|---|-----------|-----|
| A1 | It is not in the §1 protected classes. | Non-negotiable. |
| A2 | It is **derivable**: the same value can be recomputed from something still on chain, or from the event stream. | A prune that loses information is data loss, not housekeeping. |
| A3 | It is **not a claim**: no party could assert a right to it. | See §1. |
| A4 | It is older than the contract's retention window. | Time alone is not sufficient, but it is necessary. |
| A5 | Its TTL has actually lapsed, i.e. the network is no longer archiving it. | If the network is still preserving the entry, a contract-side delete destroys data the network was keeping for free. |
| A6 | No dispute, claim, or audit obligation references it. | Determined off-chain in practice; see §6. |

A record is **never** archivable merely because it is old, merely because it is
inactive, or merely because it is large.

---

## 4. The retention windows

One window per contract, chosen as "long enough that a real user or auditor
would still plausibly need it", and expressed in ledgers so it is deterministic.

| Class | Window | Rationale |
|-------|--------|-----------|
| Escrow / claim state | Until network archival | §1. Never pruned by contract code. |
| Dispute state | Until network archival, TTL renewed on every action | §1. |
| Analytics earnings history (`analytics::DataKey::Earning`) | `ANALYTICS_TTL_LEDGERS` — 1,296,000 ledgers (~90 d) | Derived from commission payouts, reconstructible from events, not a claim. |
| Analytics aggregates (`analytics::DataKey::Metrics`) | **Indefinite** | Lifetime earnings totals are the entire point of the contract. Never pruned. |
| Payer volume (`platform_config::DataKey::Volume`) | `VOLUME_TTL_LEDGERS` — 432,000 ledgers (~30 d) | Rolling input to fee tiering. A payer who transacts is renewed. |
| Registry resolution cache | `RESOLUTION_CACHE_TTL_LEDGERS` — 86,400 ledgers (~5 d) | A cache. Staleness is detected, not prevented. |
| Messaging history | `MAX_HISTORY` — 100 messages per conversation (read cap) | See §7. |
| Rate-limit records | The configured window | Transient by definition. |

---

## 5. The trap: TTL is renewed on write, not on read

This is the single most important operational consequence of the policy, and it
is the failure mode that turns "we use TTL" into "we lost a user's escrow".

`extend_ttl` fires when a record is **written**. A user who created an escrow
six months ago and has not touched it since will have that escrow's TTL lapse,
regardless of how much money it holds. The entry then leaves active state and
moves to archived state.

Consequences an operator must plan for:

- **A long pause is a data-loss event, not just an availability event.** See
  `docs/EMERGENCY_PROCEDURES.md` §4.1. A contract paused longer than
  `ESCROW_TTL_LEDGERS` (~30 days) risks losing live escrow records.
- **A "dormant" record is the most at-risk record**, not the oldest one.
- **Restoring an archived entry is not free.** It must be restored from the
  bucket list and its TTL re-bumped, which costs the user.

**Therefore:**

1. Any incident, pause, or dispute longer than the relevant TTL **must** be
   treated as a data-retention incident. Take a state backup first
   ([MAINTENANCE_WINDOWS.md §3](./MAINTENANCE_WINDOWS.md)) and record the
   ledger headroom.
2. Where a record must be kept alive regardless of activity — a disputed
   escrow, a pending claim — the write path must renew it. `escrow.open_dispute`
   already does this, replacing the base TTL with `DEFAULT_DISPUTE_TTL_LEDGERS`.
   Any new "pending" state added to a contract **must** ship with a matching
   `extend_ttl`, or it will silently expire underneath its claimant.
3. Do not "fix" a TTL expiry report by extending TTLs ad hoc. Find out why the
   write path is not firing.

---

## 6. Auditing a prune before it runs

A prune is a destructive act on a live contract, so it is gated on an
off-chain review, not just on the on-chain bounds.

- [ ] The target key is a derived, non-claimable class per §3. If there is
      any doubt, it is §1 and the answer is no.
- [ ] The retention window has passed for every record in the target range, and
      the check is on the **record's own stamp**, not on the caller's belief.
- [ ] No open dispute, pending claim, or audit hold references the range.
      Determined from the event stream and the incident tracker.
- [ ] A backup of the affected contract's view output exists
      ([MAINTENANCE_WINDOWS.md §3](./MAINTENANCE_WINDOWS.md)).
- [ ] The prune is run in a maintenance window, with a `--dry-run` first if the
      tool supports it.
- [ ] Two people approved it.

---

## 7. What this change implemented, and where

**Scope was deliberately narrow.** The issue says "component: all contracts",
but the safety constraint in §1 makes a blanket implementation wrong: adding a
prune entry point to 19 crates would put a `remove()` one careless call away
from every escrow, dispute and payment record in the workspace. The issue's own
guidance — scope it to the contracts where the precedent is clearest — is the
right call, and this change follows it.

### `analytics::prune_earnings` (#651)

```rust
pub fn prune_earnings(
    env: Env,
    artist: Address,
    upto_index: u32,
    limit: u32,
) -> Result<u32, AnalyticsError>
```

Returns the number of records removed. Every safety property required of a
prune is enforced **in the contract**, not by convention:

| Requirement | How it is enforced |
|-------------|--------------------|
| Explicit limit, never unbounded | `limit` is mandatory. `limit == 0` and `limit > PRUNE_MAX_BATCH` (100) are both rejected. |
| Bounded work, not just bounded deletes | `limit` bounds the number of indexes **scanned** as well as removed, so holes left by earlier prunes cannot turn one call into a full walk of the log. |
| Authorisation required | The stored admin must pass `require_auth`, matching every other write in the contract. |
| Never deletes protected data | A record whose `ledger` is newer than `MIN_RETENTION_LEDGERS` (= `ANALYTICS_TTL_LEDGERS`, ~90 d) is never removed. |
| Never reaches past protected data | The scan **stops** at the first in-window record rather than skipping over it. |
| Explicit scope | `upto_index` is a caller-supplied window, not "everything". |
| Aggregates untouched | Only `DataKey::Earning` is removed. `DataKey::Metrics` is never pruned. |
| Index integrity | `DataKey::EarningCount` is deliberately **not** decremented, so a later `record_earning` cannot reuse a live index. |
| Observable | Publishes `(prune)` with `(artist, removed, scanned, cutoff)`. |
| Inspectable | `get_retention_policy()` returns `(MIN_RETENTION_LEDGERS, PRUNE_MAX_BATCH)` so an operator can read the bounds off-chain. |

Unit tests in `contracts/analytics/src/tests.rs`:
`test_prune_earnings_respects_retention_window`,
`test_prune_earnings_is_bounded_by_limit`,
`test_prune_earnings_rejects_out_of_range_arguments`,
`test_prune_earnings_stops_at_a_protected_record`.

### The messaging precedent, and what is actually there

`contracts/messaging/src/types.rs` declares:

```rust
/// Maximum number of messages kept per conversation before the oldest is pruned.
pub const MAX_HISTORY: u32 = 100;
```

**The doc comment describes pruning that does not exist.** `MAX_HISTORY` is used
in exactly one place — `MessagingContract::get_messages` caps the caller's
`limit` to it:

```rust
let cap = limit.min(MAX_HISTORY);
```

So it is a **read-side page cap**, not a prune. There is no code anywhere in the
workspace that removes an old message. A conversation that has exchanged 10,000
messages still has 10,000 `DataKey::Message` entries, each with its own
persistent entry and TTL, and no code path will ever remove one.

This matters for two reasons:

1. **A reader of the constant would be misled.** The comment says the oldest is
   pruned. It is not. The comment was left alone here to keep the diff
   reviewable, but it should be corrected — either the comment, or the code.
   **This is an open follow-up, not a done item.**
2. **Messaging is the contract that most needs a real retention policy.** A
   message log is derived, non-claimable data (a soft-delete already zeroes the
   body), so it is a legitimate prune target under §3. It should get one once
   #651's mechanism has a second adopter proving the pattern.

### Contracts that still need a retention decision

None of these has been changed. Each row is a judgement call that needs an
owner, not a mechanical addition:

| Contract | Data | Recommendation |
|----------|------|----------------|
| `messaging` | `Message(Bytes, u32)`, `TypingIndicator`, `ReadReceipt` | **Prunable after the correction above.** Non-claimable, derived. Typing indicators are already logically expired at `set_ledger + 30` and are the lowest-risk prune target in the workspace. |
| `reputation` | Indexed `ReportRecord`, `Moderator` | Prune moderation reports only after an appeals window is defined. Reviews themselves: no. |
| `platform_config` | `DataKey::Volume` | TTL-only. Already correct. Do not add a prune. |
| `rate_limiter` | `RateLimitRecord` | TTL-only. Already correct. `reset_limits` is an admin override, not a retention mechanism. |
| `escrow`, `dispute_arbiter`, `donation`, `withdrawal`, `commission_agreement`, `revenue_sharing`, `creator_fund`, `subscription`, `campaign`, `competition*`, `verification` | money movement, disputes, claims | **TTL-only, never pruned.** §1. |

---

## 8. Monitoring

Pruning is a background activity that fails silently. It is monitored, not
assumed.

### On-chain

- **The `(prune)` event.** Every `prune_earnings` call emits
  `(artist, removed, scanned, cutoff)`. Index these and alert on:
  - `removed == 0` across a full prune cycle — the prune is running but never
    removing anything. Usually means the retention window was raised, or the
    wrong `upto_index` is being passed.
  - `scanned == limit` on every call, indefinitely. The batch cap is saturated,
    so the log is growing faster than the operator is pruning. **This is the
    signal that pruning has fallen behind.**
  - `cutoff` far behind the expected ledger. The operator is pruning against a
    stale retention setting.
- **`get_earning_count` growth rate.** This is the leading indicator. A rising
  count with flat `removed` means unbounded growth is winning.
- **A jump in the count for one artist** is a signal about that artist's
  activity, not about pruning.

### Off-chain

- **Storage rent per contract**, per ledger. Storage cost is the thing pruning
  exists to control; a rising rent line is the failure this whole policy is
  meant to prevent.
- **Archive rate.** If entries are being archived rather than pruned, §5 applies
  and the write paths are not renewing TTLs.
- **Bucket-list restore events**, if the RPC endpoint surfaces them. A restore
  is a record that was nearly lost.

### Runbook

Alert → check the `(prune)` event series → compare `get_earning_count` against
the expected rate → if the batch cap is saturated, raise the number of prune
calls per cycle *before* raising the batch cap. Raising `PRUNE_MAX_BATCH` makes
a single call more expensive; adding calls does not.

---

## 9. Honest status

- **`analytics::prune_earnings`, `analytics::get_earnings` and
  `analytics::get_retention_policy` have never been compiled.** The author could
  not run `cargo build`, `cargo check` or `cargo test`.
- **The four prune unit tests and the page-read test have never been
  executed.**
- The workspace does not currently build for reasons unrelated to this issue
  (see `docs/DEPLOYMENT.md` §8), so `cargo test` will fail before reaching
  this code.
- **No retention window in §4 was chosen from data.** They are the constants
  already in the source, restated and given a rationale. A maintainer with real
  usage data should revisit them — particularly the analytics 90-day window,
  which is the one number here that is a guess with a comment attached.
- **The §7 correction to `messaging::MAX_HISTORY` was not made.** It is listed
  as a follow-up rather than quietly included, because changing that comment
  changes what the codebase appears to guarantee and deserves its own review.
