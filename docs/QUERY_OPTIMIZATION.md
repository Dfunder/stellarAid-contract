# Query Optimization

> Closes **#650** — implement query optimization.
>
> Related: [STORAGE.md](./STORAGE.md), [ADR-0007](./ADRs/0007-storage-data-model-and-ttl-management.md), [RETENTION_AND_PRUNING.md](./RETENTION_AND_PRUNING.md).

---

## 0. Read this first: what was actually already there

The issue says "component: all contracts, related file `contracts/*/src/storage.rs`",
implying a shared storage layer is missing. Before writing anything, the
existing code was surveyed. **The short version is that most of the primitives
the issue is looking for already exist, and they are not in `storage.rs`.**

| Thing the issue implies is missing | Reality in this repository |
|-------------------------------------|----------------------------|
| A secondary-index framework | **Does not exist.** There is no `Index` / `index_key` abstraction anywhere. `contracts/escrow/src/storage.rs` and `contracts/platform_config/src/storage.rs` are the only two `storage.rs` files, and neither contains an index. |
| A query-result cache | **Exactly one exists**, and it is specific: `platform_config`'s resolution cache (`DataKey::ResolutionCache`, `ResolutionCacheEntry`, `RESOLUTION_CACHE_TTL_LEDGERS = 86_400`, read in `resolve_address`). There is no general cache framework. |
| Bounded / paginated reads | **Already the dominant pattern.** `search::MAX_PAGE_SIZE = 50`, `messaging::MAX_HISTORY = 100`, `analytics` `Earning(Address, u32)` + `EarningCount(Address)`, `reputation` indexed `ReportRecord`. |
| A declared page cap per contract | Already present and commented: `search` says "Hard cap on a single page of search results, bounding per-call cost." `messaging` says "Maximum number of messages kept per conversation before the oldest is pruned." |

A note on the issue's premise: it cites "~105 `Index` references". A grep of
`contracts/` for `Index` / `index_key` returns **5** matches, of which only two
are a real index — `shared::rollout::RolloutKey::FlagIndex` (a counter for
iterating feature flags) and `analytics`'s local `idx` cursor. The rest are
loop variables and prose. **The gap this issue perceives does not exist at the
scale described.** This document therefore commits to measurable targets rather
than to a new framework.

---

## 1. What this change actually did

Exactly one code change, deliberately small:

**`analytics::get_earnings(artist, from_index, limit)`** — a bounded page read
over the per-artist earnings log.

```rust
let cap = limit.min(MAX_EARNING_PAGE);          // MAX_EARNING_PAGE = 50
let end = from_index.saturating_add(cap);
```

`analytics` previously exposed only `get_earning(artist, index)` (a single
record) and `get_earning_count(artist)`. A client that wanted ten records had
to make ten invocations. The new entry point returns them in one, with the same
bound shape already used by `search` and `messaging`.

**No cache was added anywhere.** The only cache precedent in the workspace is
`platform_config`'s resolution cache, and it is not generalisable: it caches a
*derived-from-one-key* resolution, invalidated by comparing the current ledger
against a stored `resolved_ledger`, and it lives in instance storage because it
is bounded by the number of registered names. Copying that pattern onto, say,
the earnings log would require inventing a per-record TTL, a freshness rule and
an invalidation story that does not exist. That would be a framework, not an
optimisation, and it is out of scope.

**No change to `contracts/escrow/src/storage.rs` or
`contracts/platform_config/src/storage.rs`.** Both were read in full. Neither
needs a change for this issue, and both are already structured as typed
`DataKey` enums over direct `persistent()` / `instance()` accessors — which is
the correct shape. See §4 for the observations worth recording.

---

## 2. Performance targets we commit to

**These are targets, not measurements.** Nothing in this repository has been
benchmarked, and the author had no way to benchmark it — see §5. Every number
below is expressed as a property that must hold, plus how to check it, so a
maintainer can turn each one into a fact.

| # | Target | Applies to | How to check |
|---|--------|-----------|--------------|
| T1 | No view entry point reads more than a fixed, declared maximum number of ledger entries per call. | every contract | For each `pub fn` that is read-only, confirm the loop bound is `min(caller_limit, CONSTANT)` and the constant is named in the source. `search`, `messaging` and now `analytics` satisfy this. |
| T2 | Any paginated read accepts an explicit `from` cursor and an explicit `limit`. | `analytics`, `messaging`, `search`, `reputation` | Inspect the signature. All four now do. |
| T3 | A read past the end of a paginated range returns an empty page, not an error. | `analytics::get_earnings` | `test_get_earnings_returns_bounded_page` in `analytics/src/tests.rs` |
| T4 | No unbounded loop over a persistent-storage keyspace exists in any contract. | every contract | Grep for `while` / `loop` over `storage().persistent()` and confirm every one has a caller-supplied or constant bound. |
| T5 | A caller-supplied `limit` is clamped, not trusted. | `analytics`, `messaging`, `search` | Assert a `limit` above the cap returns at most the cap. |
| T6 | Append-only counters are never decremented, so an index cannot be reused. | `analytics::EarningCount` | `test_prune_earnings_respects_retention_window` in `analytics/src/tests.rs` |

T1, T2, T4 and T5 are **structural** properties: they can be checked by reading
the source, and they are what actually keeps a Soroban invocation's cost
predictable. T3 and T6 are covered by unit tests.

### What is deliberately *not* a target

- **No latency target.** Ledger execution time depends on the network's
  resource pricing, which this repository does not control. A number stated
  here would be fiction.
- **No "queries per call" count.** Same reason.
- **No claim that any query is "fast".** Nothing has been run.

---

## 3. How to actually measure this

For whoever wants real numbers, rather than the structural targets above.

**Per-call resource cost, in the test environment**

`soroban_sdk`'s test `Env` records the host function budget. Write a
benchmark-shaped test next to the existing ones in the contract's
`*_tests.rs` / `tests.rs`:

1. Seed a fixture at a realistic size — for `analytics`, a handful of artists
   with a realistic number of earnings records each.
2. Reset the budget, invoke the view once, and read the consumed amount.
3. Assert the cost does not grow when the *unrelated* portion of the fixture
   grows. **That non-growth is the real assertion.** A fixed cost is
   meaningless if it is a symptom of an empty fixture.

**On a live network**

1. Deploy to testnet.
2. Invoke the view at two fixture sizes and compare `soroban contract invoke`
   wall-clock and the transaction's resource fee.
3. `getTransaction` from the RPC endpoint reports the actual resource
   consumption. Compare against a deliberately oversized `limit` to confirm the
   clamp is doing what T5 claims.

**Round-trip count, off-chain**

The concrete win from `get_earnings` is off-chain, not on-chain: one invocation
instead of N. Measure it by counting RPC requests in a client page-load, before
and after. That is the number worth putting in a changelog, and it is the one
this change can honestly claim as a *design* improvement rather than a
*measured* one.

---

## 4. Storage-layout observations

Justified by reading the code. Each is a statement about what is there, not a
benchmark.

### `contracts/escrow/src/storage.rs`

- Clean typed `DataKey` enum over `persistent()` for records and `instance()`
  for contract-wide flags (`ReentrancyLock`, `DisputeTtlLedgers`). Correct
  shape.
- `escrow_exists` / `get_escrow` are direct keyed reads. **O(1), no scan.** The
  `Bytes` commission id is the primary key.
- `with_reentrancy_guard` uses an `instance()` lock rather than a
  `temporary()` one. It is cleared on both the success and error paths, and a
  panic reverts the whole transaction, so it is correct — but `temporary()`
  storage would be the cheaper primitive for a mutex (see
  [ADR-0007](./ADRs/0007-storage-data-model-and-ttl-management.md), which
  classes mutex locks as temporary). Worth revisiting; not changed here.
- `extend_escrow_ttl` is called on write and on dispute, with the dispute TTL
  configurable. The retention behaviour is deliberate and is documented in
  [RETENTION_AND_PRUNING.md](./RETENTION_AND_PRUNING.md).

### `contracts/platform_config/src/storage.rs`

- `DataKey` mixes instance and persistent keys in one enum, with persistent use
  confined to `DataKey::Volume(Address)`. `Volume` is TTL-managed on every
  write via `extend_ttl(&key, VOLUME_TTL_LEDGERS, VOLUME_TTL_LEDGERS)`, which
  is the correct pattern for a counter that is read far more often than the
  payer writes.
- **Everything else is instance storage**, including `FeeTiers`,
  `Promotion`, `ReferralConfig` and the registry. That is a real observation,
  not a criticism: instance storage is a single ledger entry, so
  `get_fee_tiers` is O(1) in entry count regardless of how many tiers are
  configured. It also means the whole of that configuration shares one entry's
  TTL and one entry's rent. Fine at the current scale; it becomes the first
  thing to split if the tier list ever grows large.
- The resolution cache is in instance storage, which is why it is bounded by
  the number of registered names rather than being unbounded. That constraint
  is the reason the pattern does not generalise — see §1.

### Cross-cutting

- **Every** indexed list in the workspace is an append-only
  `(key, u32)` sequence plus a separate count key: `analytics`
  `Earning`/`EarningCount`, `messaging` `Message(Bytes, u32)` +
  `Conversation.message_count`, `reputation` reports. This is a consistent,
  deliberate convention, and it is why adding a page read needed no new
  primitive.
- There is **no reverse index anywhere** — no `DataKey::EscrowsByArtist(Address)`
  or equivalent. A query like "all escrows for this artist" does not exist as a
  contract entry point and cannot be added cheaply: it would need a second
  storage key per record, written and pruned in lockstep, doubling the write
  cost of the hottest money-moving path in the workspace. **If such a query is
  needed, build it off-chain from events** ([ADR-0006](./ADRs/0006-event-driven-architecture.md)),
  which is what the correlation events in
  [CONTRACTS.md](./CONTRACTS.md) are for.
- Nothing in the workspace scans a persistent keyspace on-chain. That is a
  constraint Soroban imposes, and the codebase respects it consistently.

---

## 5. Honest status

- **Nothing here was compiled, benchmarked, or run.** The author could not
  execute `cargo build`, `cargo check`, or `cargo test`. The new
  `analytics::get_earnings` entry point has never been compiled.
- The workspace does not currently build for reasons unrelated to this issue —
  see `docs/DEPLOYMENT.md` §8. `cargo build` will fail before it reaches the
  new code.
- The two new unit tests in `contracts/analytics/src/tests.rs` have never been
  executed.
- Every number in §2 is a target to be checked, not a result. If you need a
  real number, use §3 to produce one and replace the target.
