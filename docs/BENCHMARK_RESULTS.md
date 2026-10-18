# Benchmark Results — competitions, subscription, reputation, revenue_sharing

Measured costs for the new `harness = false` stable-toolchain benchmarks added by
`LGTM-ButItsBroken` (#870 competitions, #871 subscription, #872 reputation) and
by #989 (revenue_sharing).
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

## 5. revenue_sharing (`contracts/revenue_sharing/benches/revenue_sharing_benchmark.rs`)

`record_revenue` is a looped transfer — its cost scales with the number of
recipients — so a single measurement tells you nothing. The #989 bench measures
three batch sizes plus the abort path:

```text
operation                     recipients
record_revenue                       2, 5, 20
record_revenue/insufficient          2, 5, 20
```

The deterministic figure the bench reports and enforces is the **event count**,
not wall-clock time:

| Operation | Events per call | Why |
|---|---|---|
| `record_revenue` @ N recipients | N + 1 | one `revenue` event plus one SAC `transfer` event per positive payout |
| `record_revenue/insufficient` @ N recipients | ≤ N | the transfers that completed before the failing one; the host's rollback may discard them entirely |

So cost per recipient in the success path is a flat **1 event**, and the only thing
that can make a batch "not fit" is the recipient count. The contract already caps
that at `MAX_PARTICIPANTS = 20`, and 20 recipients is ~21 events — far below the
`contract_events_size_bytes` ceiling (16,384). **No new batch limit is
required**, and the bench prints `max viable batch: 20`.

The failure path is the case nobody had measured: the source is funded for only
part of its batch, so the transfer loop fails partway through. `try_record_revenue`
is used so the abort is observed as an error rather than terminating the process,
and the call is asserted to have failed. The #989 test
`insufficient_balance_mid_batch_is_atomic` additionally asserts that no committed
state survives the abort and the source balance is intact.

**Honest status.** The bench was **written, not executed** in this environment
(the task forbade running `cargo`), so no `median_ns` figures are checked in, and
the instruction-count comparison against §1's 600M ceiling is not available on
`soroban-sdk` 21 (no `cost_estimate()`). The event budgets above are deterministic
and are additionally asserted by `event_budget_scales_with_participants` in
`src/test.rs`, so a plain `cargo test` catches an event-volume regression without
running the binary. To run it: `cargo bench -p revenue_sharing`; set
`REVENUE_SHARING_BENCH_CHECK=1` to gate on the event budgets, and optionally
`REVENUE_SHARING_BENCH_ENFORCE_NS=1` to gate on wall-clock too (not portable
across CI hardware).

## 6. commission_agreement — blocked

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

## 7. Comparison with the escrow ceilings

`PERFORMANCE_TARGETS.md` §3 budgets escrow's state-changing operations at
2–4M instructions. The operations measured here all land at or below
`subscription::withdraw`'s ~1.24M and `moderate_review`'s ~1.14M, so the escrow
range is a reasonable ceiling for the worst single-call shapes in these
contracts too. Nothing measured here is anywhere near the 600M hard ceiling; the
real constraints for these contracts are the `write_bytes` / `disk_read_entries`
list-length ceilings, which are shape analyses (see `PERFORMANCE_TARGETS.md` §2),
not instruction counts, and are unchanged by these runs.

## 8. Re-running (#997)

This section is the single place to reproduce the numbers above. Everything
needed — the command, the toolchain, the SDK version and the environment
variables that change what is measured — is listed here.

### Toolchain and SDK

Results are only comparable when produced with the same toolchain and SDK the
numbers in this document were recorded with. Record both alongside any
re-run:

- **SDK:** `soroban-sdk` 21 (`Cargo.toml` pins `soroban-sdk = "21.0.0"`). The
  mainnet ceilings quoted in §1 are from SDK 25.1.1, so they are an upper
  bound, not the current contract.
- **Toolchain:** pinned by `rust-toolchain.toml`; run `rustc --version` and
  record it. The `harness = false` benches build and run on the **stable**
  toolchain (the older `escrow`/`verification` `#[bench]` approach needs
  nightly and is out of scope here).

### Commands

The Makefile wraps the four benchmarks that this document records (one owner,
`make bench`; see issue #860):

```sh
make bench                       # every benchmark in the workspace
make bench CONTRACT=competitions # a single crate
make bench-report                # the four documented benches, one after another
```

Run them individually if you prefer the raw cargo invocation:

```sh
cargo bench -p competitions --bench competitions_benchmark
cargo bench -p subscription --bench subscription_benchmark
cargo bench -p reputation --bench reputation_benchmark
cargo bench -p revenue_sharing --bench revenue_sharing_benchmark
```

`make -n bench` prints the commands the target will run without running them.

### Environment variables that affect results

- `REVENUE_SHARING_BENCH_CHECK=1` — gate the `revenue_sharing` run on the
  deterministic **event budgets** rather than only reporting them.
- `REVENUE_SHARING_BENCH_ENFORCE_NS=1` — additionally gate on wall-clock time.
  This is intentionally **not** portable across CI hardware and should not be
  enabled in CI.
- Wall-clock budgets in general are opt-in behind an env var; the deterministic
  event-budget checks are the ones asserted by `make test`, so a plain
  `cargo test` catches an event-volume regression without running any
  benchmark binary.

### Measured vs derived

Consistent with `PERFORMANCE_TARGETS.md` §6: the `wall ns`, `cpu` and `mem`
columns in §§2–4 are **measured** (native host, `n = 50`); the ceilings they are
compared against are **derived** from the pinned SDK, and the event counts in
§5 are **deterministic and asserted**. Do not re-base the ceilings from a
re-run — re-derive them from the SDK you pin, as §1 of
`PERFORMANCE_TARGETS.md` says.

Each command prints a per-op table with wall ns, cpu and mem columns. Do not
compare wall ns across machines.