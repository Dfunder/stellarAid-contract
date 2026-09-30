# Benchmark Results — competitions, subscription, reputation

Measured costs for the new `harness = false` stable-toolchain benchmarks added by
`LGTM-ButItsBroken` (#870 competitions, #871 subscription, #872 reputation).
Companion to [`PERFORMANCE_TARGETS.md`](./PERFORMANCE_TARGETS.md), where the
budget ceilings these numbers are held against are defined.

## 1. Methodology

- Each bench is a plain binary registered as `[[bench]] harness = false` in the
  contract's `Cargo.toml`, so it builds and runs on the pinned stable toolchain
  (the older `escrow`/`verification` `#[bench]` approach needs nightly).
- Runs use the in-memory test `Env` from `soroban-sdk` 21 with `mock_all_auths`
  and a lifted budget limit (`budget().reset_unlimited()`), so a single process
  can issue every iteration.
- Per call: before the measured invocation the cumulative counters are zeroed
  with `budget().reset_unlimited()`, then the invocation runs and
  `cpu_instruction_cost()` / `memory_bytes_cost()` are read back. Each row below
  is the mean over `n = 50` calls; every call uses fresh addresses/keys so no row
  is a re-read of an earlier dropped key.
- **These are numbers from a native (non-WASM) host.** `cpu_instruction_cost()`
  counts host-native instructions and underestimates what the same code costs in
  WASM on a network; the testutils docblock warns about this. `mem_bytes_cost` is
  likewise a native-host estimate. Treat them as directional upper/lower bounds
  for ranking operations, not as the fee that a network run would bill.
- Machine: macOS, Apple Silicon, benchmark each run inside the repo with no other
  build activity. Wall-clock (`ns`) is the least portable figure and should not
  be compared across machines; instruction and memory counts are deterministic
  for a given build.

The relevant mainnet ceilings from `soroban-sdk` 25.1.1
(`InvocationResourceLimits::mainnet()`): 600,000,000 instructions,
41,943,040 memory bytes. The repository pins an earlier SDK (21.0.0); those
ceilings are the upper bound, not the current contract.

## 2. competitions (`contracts/competitions/benches/competitions_benchmark.rs`)

```text
op                 wall ns   cpu        mem
initialize             11167      28483      3171
create_competition     99419     238204     33244
submit                139598     369415     97021
vote                  309646     794957    261530
finalize               58381     227745     32467
distribute_prizes     113679     442231     61484
get_competition        23737      85032     41574
```

Notes:

- `vote` is the most expensive write path (reputation read + ranking state per
  vote). At ~0.8M of a 600M ceiling there is an order of magnitude of headroom,
  but it is also the most called user path, so it is the one to watch for the
  quadratic `flagged`-sum ranking work described for #873.
- `submit` and `create_competition` both funnel the pool escrow through the token
  contract, which is why their memory footprint is far larger than `finalize`,
  which does no token transfer.
- `distribute_prizes` ranks the full winner set (two payout transfers): roughly
  the sum of `finalize` plus costs dominated by the transfers, consistent with
  the two-transfer shape called out in §2.6 of `PERFORMANCE_TARGETS.md`.

## 3. subscription (`contracts/subscription/benches/subscription_benchmark.rs`)

```text
op               wall ns   cpu        mem
create_tier         45177      72995     11382
deposit            160295     417826    113781
subscribe          367451     675721    231500
renew              213525     476984    198977
withdraw           490505    1241452    555698
```

Notes:

- `withdraw` is the single most expensive operation measured across the three
  contracts. The path reads the full credit ledger, computes accrued entitlement
  and emits per-period credit events; its ~1.24M instructions are still ~0.2% of
  the 600M ceiling, but it is the least "free" operation per call and is the one
  a periodic sweeping caller would multiply.
- `subscribe` moves a transfer (via the token), writes subscription state, and
  extends TTL — consistent with the second-highest numbers.
- `renew` is cheaper than `subscribe` despite advancing the ledger: it reuses the
  existing subscription and only re-prices/extends, no fresh balance pull.

## 4. reputation (`contracts/reputation/benches/reputation_benchmark.rs`)

```text
op                  wall ns   cpu        mem
submit_review         45667      57499      8202
get_review            11826      43509     10556
report_review        266737     348968    110845
moderate_review      527929    1135255    361829
```

Notes:

- `moderate_review` is the costliest reputation path: it loads the reported
  review, re-sums stored reviews, applies the moderation outcome across the
  artist's review log (re-`persistent().get` per stored review), and then
  re-`report`s. Its memory footprint (~362K) is the highest single-call byte cost
  in these runs after `subscription::withdraw`.
- `report_review` includes the O(reviews-so-far) dedup scan described in §2.2 of
  `PERFORMANCE_TARGETS.md` — at ~349K instructions with a near-empty log it is
  already 6x `submit_review`, which is why capping the review-count scan matters.
- `get_review`/`submit_review` are flat writes/reads; no surprises.

## 5. commission_agreement — blocked

`contracts/commission_agreement/benches/commission_agreement_benchmark.rs`
(create_agreement, propose_milestone, approve_milestone, distribute_batch) is
written and registered, but **cannot run**: the crate does not compile on
`main` (22 errors — missing `DataKey::RateLimiter`/`types::RateLimitKey`,
duplicate error discriminants 10–13 in `errors.rs`, duplicate imports,
use-after-move, an oversized `#[contracterror]` symbol). This is the pre-existing
breakage documented in §6 of `PERFORMANCE_TARGETS.md`; it was deliberately not
fixed here (out of scope for #870–#872), so `distribute_batch` — the repository's
highest single-call transfer loop (`MAX_BATCH = 25`) — still has no measurement.
The bench is written against the current API and is expected to become runnable
unchanged once the crate compiles.

## 6. Comparison with the escrow ceilings

`PERFORMANCE_TARGETS.md` §3 budgets escrow's state-changing operations at
2–4M instructions. The operations measured here all land at or below
`subscription::withdraw`'s ~1.24M and `moderate_review`'s ~1.14M, so the escrow
range is a reasonable ceiling for the worst single-call shapes in these
contracts too. Nothing measured here is anywhere near the 600M hard ceiling; the
real constraints for these contracts are the `write_bytes` / `disk_read_entries`
list-length ceilings, which are shape analyses (see `PERFORMANCE_TARGETS.md` §2),
not instruction counts, and are unchanged by these runs.

## 7. Re-running

```sh
cargo bench -p competitions --bench competitions_benchmark
cargo bench -p subscription --bench subscription_benchmark
cargo bench -p reputation --bench reputation_benchmark
```

Each prints a per-op table with wall ns, cpu and mem columns. Do not compare wall
ns across machines; do not re-base the ceilings from them either — re-derive the
ceilings from the SDK you pin, as §1 of `PERFORMANCE_TARGETS.md` says.