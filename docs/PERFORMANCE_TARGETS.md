# Contract Performance Targets

Documents target execution-cost budgets for the core contract operations benchmarked in [`contracts/escrow/benches/escrow_benchmark.rs`](../contracts/escrow/benches/escrow_benchmark.rs), so a benchmark run has something concrete to compare against instead of just "did it get slower." Related to #639.

## Why budgets, not raw wall-clock times

`cargo bench` wall-clock numbers vary with the machine running them and aren't comparable across CI runs on different hardware. Soroban's own cost model — CPU instructions and ledger I/O bytes, both reported by the SDK's simulation/testbench — is deterministic for a given WASM build and input, and is what actually determines the on-chain transaction fee. Targets below are expressed in those terms.

## Targets

| Operation | CPU instructions (budget) | Ledger I/O (budget) | Rationale |
|---|---|---|---|
| `create_escrow` | ≤ 3,000,000 | ≤ 2 persistent writes, ≤ 2 cross-contract calls | One escrow record write, one TTL extend, two config lookups (`get_fee_b`, `get_usdc`). |
| `release_payment` | ≤ 4,000,000 | ≤ 1 persistent write, 2 token transfers | Fee-split arithmetic plus two token transfers (artist + platform wallet) is the dominant cost over `create_escrow`'s one transfer. |
| `refund_client` | ≤ 3,000,000 | ≤ 1 persistent write, 1 token transfer | Same shape as `create_escrow` but no fee-split arithmetic. |
| `expire_escrow` | ≤ 2,000,000 | ≤ 1 persistent write | No token transfer at all — the cheapest of the four state-changing operations. |

These are starting budgets based on the operations' own storage/cross-contract-call shape above, not measurements from a specific benchmark run (this repo has no baseline numbers checked in yet). Treat them as the ceiling to tighten once real benchmark output exists, not as already-verified figures.

## Regression detection

A benchmark run should fail (or at minimum warn loudly) when an operation's measured CPU instructions or I/O count exceeds its budget above. `escrow_benchmark.rs`'s existing `Bencher`-based benches report timing, not cost directly; wiring in the SDK's cost-tracking API (`env.cost_estimate()` / `env.budget()`, exposed by `soroban-sdk`'s test utilities) to assert against the table above is the concrete next step — left for a follow-up focused on the benchmark file itself, since this document is scoped to defining the targets, not implementing the check.

## Updating this table

Whenever a contract change measurably shifts one of these operations' cost (a new storage field, an added cross-contract call, etc.), update the corresponding row and note the reason in the same PR — this table should always reflect what the current code actually costs, not what it cost when this document was written.
