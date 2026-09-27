# Contract Performance Targets

Targets the execution-cost budgets that contract operations should be held to, and the
code-level notes behind them. Closes **#875** (gas optimisation notes for the non-escrow
contracts). Related: [`WASM_OPTIMIZATION.md`](./WASM_OPTIMIZATION.md) (size budget),
[`QUERY_OPTIMIZATION.md`](./QUERY_OPTIMIZATION.md) (earlier bounded-read work),
[`UPGRADE_AND_ROLLBACK.md`](./UPGRADE_AND_ROLLBACK.md).

Every target below is derived from a contract's own code shape, not from a benchmark run,
because this repository cannot currently be built (§6). That is stated up front rather than
buried, because it changes how the numbers should be used: they are ceilings to tighten once
real output exists, not measurements.

## 1. What a target is measured against

`cargo bench` wall-clock numbers vary with the machine running them and are not comparable
across CI hardware. The figures that actually determine an on-chain fee are deterministic for
a given WASM build and input, and are the ones budgeted here.

The ceilings come from the network, not from this repository. As shipped in `soroban-sdk`
25.1.1, `InvocationResourceLimits::mainnet()` (`soroban-sdk/src/testutils/cost_estimate.rs:138-152`)
is:

| Resource | Mainnet ceiling | Notes |
|---|---|---|
| `instructions` | 600,000,000 | The usual binding constraint for compute. |
| `write_entries` | 50 | **Tight.** Most contracts below are safe here; the audit contract is not. |
| `write_bytes` | 132,096 | **Tighter than it looks** — see the note below. |
| `disk_read_entries` | 100 | **Tightest.** Loop-over-storage-keys code exceeds this at small N. |
| `ledger_entries` | 100 | Billed on the union of read and written entries. |
| `disk_read_bytes` | 200,000 | |
| `mem_bytes` | 41,943,040 | |
| `contract_events_size_bytes` | 16,384 | |
| `max_contract_data_entry_size_bytes` | 65,536 | One stored value. |
| `max_contract_data_key_size_bytes` | 250 | One key. |

This repository pins `soroban-sdk = "21.0.0"`, an earlier protocol. Treat the table as the
upper bound these budgets are written against, and re-derive if the SDK moves.

**The `write_bytes` trap.** `write_entries = 50` looks generous until you notice that a
single "entry" can be very large. Contracts here store append-only lists as one `Vec` in one
`persistent()` key, and every append reads the whole list, pushes, and writes it all back.
That is one write entry and an ever-growing number of write bytes. A 5,000-entry history is
comfortably one entry and comfortably past `write_bytes`. Counting entries is not enough;
this is why the targets below are written in terms of list length as well as counts.

**The `disk_read_entries = 100` trap.** Code that loops `while i < count { storage().get(&Key(i)) }`
costs one read entry per iteration, so it blows the read budget at N ≈ 100 — a very small N
for a contract that has been running for a month. The contracts listed in §2.2 all do this.

Targets are stated as a fraction of the relevant ceiling, with the list length that the
fraction assumes. A target with no list length next to it is a flat-cost target.

## 2. High-risk shapes, by contract

Ranked by how badly a single invocation can go wrong, worst first. Each entry is a code fact
with a `file:line`, not an estimate.

### 2.1 Caller-supplied `limit` with no ceiling — DoS-shaped

The sharpest problem in the repository. These getters take an `offset`/`limit` straight from
the caller, never clamp it to a maximum, and derive the loop bound from a **monotonic counter
that has no cap of its own**:

| Contract / entry point | Counter | Loop | Reads per call |
|---|---|---|---|
| `nft::get_transfer_history` (`nft/src/lib.rs:308-333`) | `TransferCount` | `:321-332` | up to `min(offset+limit, count)` |
| `mentorship::get_feedback` (`mentorship/src/lib.rs:496-516`) | `FeedbackCount` | `:505-514` | `count` |
| `licensing::get_usage_entries` (`licensing/src/lib.rs:492-518`) | usage count | `:505` | up to `min(offset+limit, count)` |
| `dao::get_history` (`dao/src/lib.rs:528-543`) | `HistoryCount` | `:536` | up to `min(offset+limit, count)` |
| `ecosystem_funding::get_outcomes` (`ecosystem_funding/src/lib.rs:450-471`) | outcome count | `:458` | up to `min(offset+limit, count)` |
| `reputation::get_reviews` (second contract, `reputation/src/lib.rs:786-788`) | review count | — | whole list |

`nft::get_transfer_history` is the clearest example and is worth reading as the template for
the rest:

```rust
// nft/src/lib.rs:321-323
let end = (offset + limit).min(count);
let mut i = offset;
while i < end {
```

Two separate problems. The loop bound is `min(offset + limit, count)`, and `count` grows with
every transfer of the token, so a caller passing `limit = 1_000_000` forces up to a million
`persistent().get` calls in one invocation — against a `disk_read_entries` ceiling of 100.
Separately, `offset + limit` is **unchecked `u64` addition**: in a release build two large
caller values wrap, and `end` can end up *below* `offset`. The `.min(count)` does not protect
against that.

**Recommendation.** Clamp `limit` to a constant before computing `end`, use
`saturating_add` for the bound, and take the count as a hard upper bound. `shared::pagination`
already provides the clamp (`effective_limit` / `MAX_PAGE_SIZE`); the missing half is a
paged variant for `nft`, `licensing`, `dao` and `ecosystem_funding`, which unlike the
contracts touched by #876 have none.

`mentorship::get_feedback` is the same shape with no `limit` parameter at all, so it is
unconditionally O(count) — there is no way for a caller to ask for less. `get_feedback_page`
(#876) bounds the response but still calls this function first (`mentorship/src/lib.rs:530`),
so it inherits the full walk.

### 2.2 Uncapped append-only lists

A list that is appended to on every state change and never trimmed has no cost ceiling. The
`push_history`-shaped code is read-whole / push / write-whole, so every append costs O(len) in
both instructions and write bytes.

**Trimmed, but by configuration rather than by a code constant** — safe today, one config
change away from not being:

| Contract | List | Cap source |
|---|---|---|
| `verification` | `History`, `BadgeHistory` | admin `HistoryLimit` (`verification/src/lib.rs:87-90`, `:117-120`) |
| `subscription` | `Payments` | admin `HistoryLimit` (`subscription/src/lib.rs:97-100`) |
| `creator_fund` | `Growth`, `Allocations` | admin `HistoryLimit` (`creator_fund/src/lib.rs:75`, `:86-88`) |
| `revenue_sharing` | `History` | admin `HistoryLimit` (`revenue_sharing/src/lib.rs:95-102`) |
| `competitions` | `History` | admin `HistoryLimit` (`competitions/src/lib.rs:147`, `:395-400`) |
| `recruitment` | `Applicants` | admin `MaxApplicants` (`recruitment/src/lib.rs:133-135`, `:227-229`) |

Every one of these `initialize` paths rejects `0` but has **no upper bound**, so an admin can
set a limit large enough to make the associated appends exceed `write_bytes`. Recommend a code
constant ceiling on the configured value at `initialize` time.

`revenue_sharing::Splits` is the one list here bounded by a real code constant:
`MAX_PARTICIPANTS = 20` (`revenue_sharing/src/types.rs:4`, enforced `lib.rs:59-61`).

**Not trimmed at all:**

| Contract | List | Push | Notes |
|---|---|---|---|
| `donation` | `DonationHistory` (`lib.rs:17`) | `:113` | Uncapped, one per donation per donor. |
| `donation` | `CampaignDonations` (`lib.rs:18`) | `:108` | Uncapped, and **global per campaign**, so it is shared by every donor. |
| `withdrawal` | `WithdrawalsByCampaign` (`lib.rs:16`) | `:94` | Uncapped; each record is also stored separately at `Withdrawal(id)`, so it is stored twice. |
| `ecosystem_funding` | `Milestones` (`lib.rs:110`) | `:196` | Written from a caller-supplied `Vec` through an uncapped loop (`:167-170`). |
| `mentorship` | `Milestones` (`lib.rs:97`) | `:180` | Same, via a caller-supplied `Vec` (`:179-188`). |
| `licensing` | `Usage` (`lib.rs:114`) | `:356-361` | Append-only usage log. |
| `reputation` (2nd contract) | `ReviewsForArtist` (`lib.rs:551-552`) | `:551` | No cap. **UNVERIFIED** — file does not parse, see §6. |
| `reputation` (1st contract) | `Report` dedup scan | `:138-145` | `for i in 0..count` over reports-so-far, O(n) per report. **UNVERIFIED.** |
| `verification` | `BadgeTypes` (`lib.rs:132-134`) | `:133` | No trim, but bounded in practice by the `BadgeType` enum's variant count. |
| `dao` | `MultiSigSigners` (`lib.rs:113`) | — | Caller-supplied `Vec<Address>`, no length check at write. |
| `search` | `AllArtists` (`types.rs:76`, **instance**) | `lib.rs:210` | One entry per distinct artist, no removal path. |
| `platform_config` | `FeeTiers` (`storage.rs:27`, **instance**) | — | No cap; `upsert_fee_tier` scans and rebuilds the whole list (`:177-199`). **UNVERIFIED.** |

### 2.3 `RolloutKey::FlagIndex` — one uncapped loop reachable from every contract

This is the most replicated defect in the repository. More than twenty contracts carry a
generated tail of `health_check` / rollout entry points built on `shared/src/rollout.rs`, and
all of them inherit the same unbounded write loop.

- `set_feature_flag` (`shared/src/rollout.rs:169-183`) scans the whole `FlagIndex` to dedup
  (`:175-180`) and then pushes (`:182`). Cost is O(flags set so far).
- `disable_all_flags` (`shared/src/rollout.rs:220-227`) writes every flag off, one
  `instance().set` per entry (`:223-225`).
- `apply_rollback` (`:230-236`) is what calls it.
- `trigger_rollback` (`:261-263`) sets `Paused`, and reaches the same loop.

`FlagIndex` (`shared/src/rollout.rs:34`) is a `Vec` with **no cap anywhere**. A
`set_feature_flag` call costs O(N) writes, and a rollback costs O(N) writes — and unlike the
lists in §2.2 this is in *instance* storage, so it is re-read and re-written on every
invocation of the contract, not once per history entry.

Worse, it is reachable from a *read-shaped* entry point. `health_check` looks like a view but
calls `maybe_auto_rollback` (`shared/src/rollout.rs:242`) whenever the error rate crosses the
threshold, which reaches `apply_rollback` and therefore the loop. A caller polling health
cannot tell that they are triggering a write storm.

**Recommendation.** Cap `FlagIndex` length in `set_feature_flag` and reject beyond the cap with
a typed error, the same way `revenue_sharing` caps `MAX_PARTICIPANTS`. A flag registry with
more than a couple of dozen entries is a design smell anyway. Until that lands, treat
`set_feature_flag` and `health_check` as uncapped in any budget.

### 2.4 Write amplification

| Contract / entry point | Writes | Where |
|---|---|---|
| `audit::write_entry` | up to **10 persistent** + 10 `extend_ttl` | `audit/src/storage.rs:90-123` |
| `revenue_sharing::record_revenue` | up to **23** | 1 + N `Earnings` (N ≤ 20) + `Agreement` + `History`, `lib.rs:209-290` |
| `search::search` | 1 write, but **N reads** | `search/src/lib.rs:299-307` |
| `donation::donate` | 3 | `lib.rs:109`, `:114`, `:118` |
| `creator_fund::execute_allocation` | 4 | `lib.rs` `:384` and surrounds |
| `recruitment::apply_for_job` | 4 | `lib.rs:241`, `:250`, `:253` |
| `dao::create_proposal` | 4 | `lib.rs:226`, `:560`, `:585-586` |
| `ecosystem_funding::create_program` | 4 | `lib.rs:189-190`, `:208`, `:482` |

`audit::write_entry` is the one to watch against `write_entries = 50`: one call writes ten
entries and issues ten `extend_ttl` calls. It is still inside the ceiling, so it is not a
budget violation today, but it is the highest single-call write count in the repository and it
is not covered by any benchmark.

### 2.5 Quadratic validation

- `revenue_sharing::validate_splits` (`lib.rs:70-74`) is O(n²) over participants. Bounded at
  20, so ~400 comparisons — acceptable, but it is why the 20 cap must not be raised casually.
- `multi_sig::validate_config` (`lib.rs:107-113`) is O(n²) dedup. Bounded by
  `MAX_SIGNERS = 10` (`types.rs:7`) — fine.
- `search::sort_listings` (`lib.rs:125-143`) is O(n²) over the full match set, and
  `search::search` (`:273-333`) does one `persistent().get` per artist (`:299-307`) over the
  whole uncapped `AllArtists` list before sorting. This is the single most expensive read path
  in the repository: unbounded reads **and** quadratic CPU in one invocation.

### 2.6 Token transfers and cross-contract calls

Per invocation, and none of these are in a loop except where noted:

| Contract | Transfers | Cross-contract invokes |
|---|---|---|
| `revenue_sharing::record_revenue` | up to **20, in a loop** (`lib.rs:280-285`) | 0 |
| `commission_agreement::distribute_batch` | up to **25, in a loop** (`lib.rs:1220-1225`, `MAX_BATCH = 25`) | 0 |
| `dispute_arbiter::partial_resolve` | 2 (raw invokes, `lib.rs:222-255`) | **5** (`:222`, `:228`, `:233`, `:243`, `:255`) |
| `donation::donate` | 1 (`lib.rs:88`) | 2 (`:82`, `:120`) |
| `withdrawal::approve_withdrawal` | 1 (`lib.rs:137`) | 1 (`:117`) |
| `creator_fund::contribute` / `execute_allocation` | 2 each | 0 |
| `escrow::atomic_escrow_to_commission` | 0–2 | 4 |
| `escrow::release_payment` / `partial_release` | 2 | 3 |

The two looped-transfer paths are the ones with a real ceiling risk. `record_revenue`'s 20 is
indirectly bounded by `MAX_PARTICIPANTS`; `distribute_batch`'s 25 is a checked code constant
(`commission_agreement/src/lib.rs:1184`). Both are defensible. They should be **benchmarked**,
which neither currently is — they are the most expensive operations in the repository by
instruction count and neither has a benchmark.

`dispute_arbiter::partial_resolve` at five cross-contract invokes in one call is the outlier on
the invoke side, and none of the five are in a loop.

### 2.7 Persistent TTL is mostly not extended

A `persistent()` entry written without `extend_ttl` gets the default network TTL and then
expires. That is a data-loss and expiry-bug risk as much as a cost one, and it is worth fixing
in the same pass as anything here.

**Do extend:** `analytics` (`ANALYTICS_TTL_LEDGERS = 1_296_000`, `lib.rs:26`, applied `:126`,
`:132`, `:521`), `audit` (`AUDIT_TTL_LEDGERS = 5_184_000`, `storage.rs:28`), `multi_sig`
(`MULTISIG_TTL_LEDGERS = 1_296_000`, `lib.rs:79`, applied `:142-145`, `:284-288`),
`rate_limiter` (`lib.rs:239-243`), `escrow` for `DataKey::Escrow` only (`lib.rs:68-72`),
`campaign` partially, `platform_config` for `DataKey::Volume` (`storage.rs:243-245`).

**Do not extend:** `creator_fund`, `dao`, `dispute_arbiter`, `donation`, `ecosystem_funding`,
`licensing`, `mentorship`, `messaging`, `nft`, `recruitment`, `revenue_sharing`,
`search`, `subscription`, `verification`, `withdrawal`, and every key of
`commission_agreement`.

Two specific gaps worth calling out:

- `escrow::DataKey::AtomicCommit` is written at `cross_contract.rs:69` with no `extend_ttl`
  and no removal path, so an atomic commit can expire mid-flight. `DataKey::Escrow` right next
  to it *is* extended.
- `campaign` extends on `create_campaign` (`:276`), `update_raised` (`:337`) and
  `finalize_withdrawal` (`:364`) but not on `update_campaign_status` (`:310-312`) or
  `reject_campaign` (`:392-394`), so a campaign that is only ever rejected loses its record
  before a viewer can read it.
- `messaging` stores `Message`, `ReadReceipt` and `TypingIndicator` with no `extend_ttl` at
  all, and `send_message` is the only write to `Conversation` (`lib.rs:176`).

## 3. Escrow targets

Retained from the original document. These are the target budgets for the operations
benchmarked in `contracts/escrow/benches/escrow_benchmark.rs`.

| Operation | CPU instructions (budget) | Ledger I/O (budget) | Rationale |
|---|---|---|---|
| `create_escrow` | ≤ 3,000,000 | ≤ 2 persistent writes, ≤ 2 cross-contract calls | One escrow record write, one TTL extend, two config lookups (`get_fee_b`, `get_usdc`). |
| `release_payment` | ≤ 4,000,000 | ≤ 1 persistent write, 2 token transfers | Fee-split arithmetic plus two token transfers (artist + platform wallet) is the dominant cost over `create_escrow`'s one transfer. |
| `refund_client` | ≤ 3,000,000 | ≤ 1 persistent write, 1 token transfer | Same shape as `create_escrow` but no fee-split arithmetic. |
| `expire_escrow` | ≤ 2,000,000 | ≤ 1 persistent write | No token transfer at all — the cheapest of the four state-changing operations. |

These are starting budgets based on the operations' own storage/cross-contract-call shape, not
measurements from a specific benchmark run (this repo has no baseline numbers checked in yet).
Treat them as the ceiling to tighten once real benchmark output exists, not as
already-verified figures.

Note that `escrow` is the best-instrumented contract in the repository and still has no
benchmark that runs. `contracts/escrow/benches/escrow_benchmark.rs` uses `#[bench]` /
`test::Bencher`, which needs a nightly toolchain, and no `[[bench]]` target is registered in
`contracts/escrow/Cargo.toml`, so that file is not compiled at all. The approach taken for
#873 — a `harness = false` target on stable, with an opt-in budget gate — is the one to copy
for the operations named in §2.

## 4. Optimisation backlog, in priority order

Ordered by (likelihood of being hit) × (blast radius), not by effort.

1. **Clamp caller-supplied `limit` in the six getters in §2.1**, and switch the `offset + limit`
   bounds to `saturating_add`. This is the only item on the list that is a remotely-triggerable
   resource-exhaustion path, and the `nft` one also has an unchecked-addition bug.
2. **Cap `RolloutKey::FlagIndex` in `shared/src/rollout.rs`** (§2.3). One change fixes every
   contract that carries the rollout block, and it currently makes a read-shaped entry point
   capable of an unbounded write loop.
3. **Trim the uncapped lists in §2.2**, starting with `donation::CampaignDonations` (global per
   campaign, shared by all donors) and `withdrawal::WithdrawalsByCampaign` (which also stores
   every record twice).
4. **Add code-constant ceilings to the admin-configured caps** in §2.2, and bound
   `search::search` — the only path in the repository with both unbounded reads and quadratic CPU.
5. **Add `extend_ttl` to the keys in §2.7**, `escrow::AtomicCommit` first, since that one is a
   correctness bug rather than a tuning question.
6. **Benchmark `revenue_sharing::record_revenue` and `commission_agreement::distribute_batch`**
   (§2.6). These are the two looped-transfer operations and are more expensive than anything
   currently benchmarked.
7. Add `shared::pagination` variants for the four contracts in §2.1 that have none.

## 5. Regression detection

A benchmark run should fail, or at minimum warn loudly, when an operation's measured
instructions or I/O count exceed the budget in §1. The verification benchmark added for #873
(`contracts/verification/benches/verification_benchmark.rs`) is the reference implementation:
a `harness = false` target on stable, a deterministic event budget enforced by default in
`cargo test` via `src/test.rs`, and wall-clock budgets gated behind an environment variable
because they are not portable across CI hardware.

Two limitations to keep in mind when copying it:

- It runs against the in-memory test `Env` with a native contract, so its `median_ns` is not an
  instruction count. `env.cost_estimate()` gives real instruction and I/O numbers, but it is
  **not available in `soroban-sdk` 21**, which this repository pins — that is why the benchmark
  reports event counts instead, and why real budget enforcement waits on an SDK upgrade.
- `env.events().all()` returns the host's whole event log, not just the last invocation, so an
  event count only means anything as a difference taken around the call. The benchmark and the
  test both do this. An earlier revision asserted an absolute count of 1 and would have failed
  on the second call.

`env.cost_estimate().resources()` on `soroban-sdk` ≥ 22 is the concrete next step for turning
§2's fractions into enforced numbers.

## 6. Honest status

- **Nothing in §2 or §3 is a measurement.** No contract in this repository has been built, so
  no target here has been checked against a real invocation. The code facts and `file:line`
  citations are real; the budgets expressed against them are derived, not observed.
- **The workspace does not currently build.** Twelve files under `contracts/` have unclosed
  delimiters on `upstream/main`, including `contracts/reputation/src/lib.rs`,
  `contracts/platform_config/src/*`, `contracts/escrow/src/lib.rs` and
  `contracts/commission_agreement/src/lib.rs`. See [`DEPLOYMENT.md`](./DEPLOYMENT.md) §8. This
  is pre-existing and unrelated to #875. Two files matter for §2: `reputation/src/lib.rs` has
  two separate contracts concatenated into one file (`ReputationContract` ends mid-body at
  `:644`, a second contract starts at `:344`), and `commission_agreement/src/lib.rs` has
  `get_team_members` unclosed at `:678-685`, which means every entry point from line 690 down
  is textually nested inside it and is not actually a member of the `#[contractimpl]` block.
- **Facts read from an unparseable file are marked UNVERIFIED** in §2.2. That covers
  `reputation` and `platform_config` entirely. `escrow::correlation_tests.rs` may be balanced
  after all; the earlier reading was not reproduced.
- **`commission_agreement` is included in §2.6 for completeness** but is not currently
  buildable, and has errors beyond the delimiter: `DataKey::RateLimiter` and
  `types::RateLimitKey` are referenced but do not exist in its `types.rs`, and two
  `env.events().publish` calls are given three arguments where the SDK takes two.
- **The `soroban-sdk` ceilings in §1 are from 25.1.1.** The repository pins 21.0.0, an earlier
  protocol. Re-derive before relying on the headroom.

## 7. Updating this document

Whenever a contract change measurably shifts one of these operations' cost — a new storage
field, an added cross-contract call, a removed list trim — update the corresponding row and note
the reason in the same PR. This document should reflect what the current code actually costs,
not what it cost when it was written.
