# CI baseline status

## Current state (this checkout)

- Active workflow: `.github/workflows/typescript-ci.yml` (TypeScript/bindings
  and SDK utility checks).
- No `.github/workflows/ci.yml` Rust job pipeline with `continue-on-error` flags
  or a BASELINE comment block is present in this checkout.
- A `rust-check` job (`cargo check --workspace --all-targets --locked`) has been
  added to `typescript-ci.yml` as a fast workspace compile gate.

## Reconciliation notes

- Issues #921/#922 reference a Rust CI workflow (`ci.yml`) with blocking/`continue-on-error`
  jobs and a compile-failure BASELINE list. Because that workflow and baseline
  block are not present in the current repository state, there are no
  `continue-on-error: true` lines to remove and no stale baseline entries to
  update here.
- When Rust lint/test/wasm/coverage CI jobs and a baseline block are
  (re)introduced, reconcile the baseline against actual reproducible failures
  and flip/remove `continue-on-error` only as each gate passes on `main`.
