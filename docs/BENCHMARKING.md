# Benchmarking

This repository includes Rust benchmarks for workspace crates (for example,
`contracts/escrow/benches/escrow_benchmark.rs`).

## Targets

- `make bench` — runs `cargo bench --workspace`.
- `make bench-compare` — runs `cargo bench --workspace`, saves results to
  `.bench/results.txt`, keeps the previous run in `.bench/results.prev.txt`,
  and prints a unified diff against the previous run when available. The first
  run establishes a baseline.

Benchmark result files under `.bench/` are local artifacts and are ignored by
git.
