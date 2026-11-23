# Contributing to StellarAid contracts

## Before you push

Run the local quality gate (format check, clippy, tests) with a single command:

```bash
make check
```

`make check` is composed from the same underlying targets as CI-style checks and
does **not** modify the working tree:

- `make fmt-check` — `cargo fmt --all -- --check`
- `make lint` — `cargo clippy --workspace --all-targets -- -D warnings`
- `make test` — `cargo test --workspace --no-fail-fast`

Use `make fmt` to apply formatting locally when needed.

`make build` runs the locked WASM build
(`cargo build --workspace --target wasm32-unknown-unknown --release --locked`)
used by the CI wasm job. The Makefile and `.github/workflows/ci.yml` must stay in
sync: when you change a Cargo command or flag, change both in the same PR.
`RUST_TEST_THREADS=1` is exported by the Makefile and set in CI for the same
reason — Soroban's test host shares one ledger and parallel runs produce
order-dependent failures.

## SDK boundary review checklist

Code under `sdk/` crosses a network or wallet boundary. Reviewers reject:

- `response.json() as Promise<SomeInterface>` (or any `as Promise<T>` cast on a
  fetch result) — JSON has no `bigint`, and a cast does not validate anything.
  Use `fetchJson(url, validator)` from `sdk/src/net/validate.ts` instead.
- `fetch(url + '/' + id)` — encode path segments with `encodeURIComponent` (or
  use `buildUrl`, which does it for you).
- `fetch` without a timeout — `fetchJson` supplies `AbortSignal.timeout`.
- `error?.message?.includes(...)` on an `unknown` error — narrow with
  `instanceof Error` and map to a typed error such as `SorobanRejectionError`.
- `Promise<any>` / `any` in exported signatures — use a generic `<T>`.

`npm run typecheck --prefix sdk` enforces `noImplicitAny`; the items above are
the ones a type checker cannot catch.

## Deployment preflight

`scripts/preflight.sh` (invoked by `make preflight` where available) runs
repository gates and deploy-environment checks. It includes a fast compile gate
(`cargo check`) before slower checks/build steps so compile errors are caught
early.

## Workspace crate coverage decisions

- **`worker/` and `multi_wallet_session/`**: these crates are not present in this
  checkout and are not registered as Cargo workspace members. They are therefore
  not compiled by `cargo check --workspace`/workspace CI jobs here. If/when these
  crates are added to the repository, they must be either (a) registered as
  workspace members and covered by workspace checks/lint/tests/CI, or (b)
  covered by a dedicated CI job with an explicit exclusion/reason recorded in
  `Cargo.toml` (issues #923, #861–#864 context).
- **`tests/wallet_integration/`**: this test crate is not present in this
  checkout and is not a workspace member. It is not run by
  `cargo test --workspace`. When added, register it as a workspace member, ensure
  `cargo test --workspace` runs it, keep any network-dependent tests `#[ignore]`d
  by default with documented opt-in, and pass workspace clippy gates (issue #924).

These decisions are also recorded in `Cargo.toml` notes so workspace membership
and coverage expectations remain explicit.
