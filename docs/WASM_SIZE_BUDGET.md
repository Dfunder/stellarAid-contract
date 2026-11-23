# WASM size budget, Docker deps layer, TTL bumps, DAO pagination

## Release profile and size budget (#1118)

The workspace `Cargo.toml` now has a `[profile.release]` tuned for Soroban: `opt-level = "z"`,
`lto`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`, `overflow-checks = true`.
Smaller WASM lowers per-transaction cost. `make wasm-size` prints the size of every built
contract WASM. `Cargo.toml.optimization` is not present in this checkout, so nothing to delete.

CI follow-up: fail the wasm job when a contract exceeds its budget (start at 64 KiB per
contract and tighten after the first measurement).

## Dockerfile deps layer (#1117)

No `Dockerfile` exists in this checkout. When it is added, copy the root `Cargo.toml` and
`Cargo.lock`, then the member manifests generated from `cargo metadata --no-deps`, so a new
member can never silently miss the fetch layer. State this invariant in a Dockerfile comment.

## TTL bumps on persistent records (#1119)

Use one shared constant, for example `PERSISTENT_TTL_THRESHOLD` and `PERSISTENT_TTL_EXTEND_TO`,
and call `env.storage().persistent().extend_ttl(&key, THRESHOLD, EXTEND_TO)` on every read and
write of each persistent key. Only the `nft`, `licensing`, `dao`, `ecosystem_funding`,
`mentorship` and `commission_agreement` crates are affected, and none are in this checkout.
Add a ledger-advance test per crate (more than 90 days).

## DAO `get_history` pagination (#1120)

Add `get_history_page(cursor, limit)` using `contracts/shared/src/pagination.rs`, clamp `limit`
to a maximum, and keep `get_history` as a capped, documented-deprecated shim. Test that pages
return at most `limit` entries and that cursors stay stable as history grows.
