# Contributing

## Local checks mirror CI

The root `Makefile` and `.github/workflows/ci.yml` run the **same** Cargo
invocations. `Makefile` declares the flag sets once; the workflow spells out
the same commands. If you change a flag, change both in the same PR.

| Local | CI step | Command |
|---|---|---|
| `make build` | Build contracts | `cargo build --workspace --target wasm32-unknown-unknown --release --locked` |
| `make test` | Test | `cargo test --workspace --no-fail-fast --locked` |
| `make lint` | Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |

Both export `RUST_TEST_THREADS=1`: Soroban's test host shares one ledger and a
parallel runner produces order-dependent failures.

Run `make lint test` before opening a PR. A local green must mean CI green.

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
