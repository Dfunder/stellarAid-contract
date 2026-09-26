# Soroban SDK Version Compatibility

Tracks which `soroban-sdk` versions this workspace has been built and verified against, so upgrading the SDK is a deliberate, documented decision rather than a silent `cargo update`. Related to #645.

## Current pinned version

The workspace root [`Cargo.toml`](../Cargo.toml) pins:

```toml
soroban-sdk = { version = "21.0.0" }
```

Every contract crate inherits this via `soroban-sdk.workspace = true`, so there is exactly one place to bump the version for the whole workspace.

## Version matrix

| SDK version | Status | Notes |
|---|---|---|
| 21.0.x | **Supported (current)** | All contracts in `contracts/*` build and are developed against this line. |
| 20.x | Untested | Predates several APIs this workspace relies on (e.g. `env.deployer().update_current_contract_wasm`, the `#[contracterror]` discriminant conventions used across every contract's error enum). Not verified; likely requires code changes, not just a version bump. |
| 22.x+ | Untested | Not yet evaluated. Before upgrading, check the SDK's own changelog for storage-layout or auth-model changes, since this workspace stores non-trivial `contracttype` enums directly as persistent-storage keys (see e.g. `contracts/withdrawal/src/lib.rs`'s `DataKey`) — any change to how the SDK encodes those would be a breaking migration, not just a recompile. |

## Why this isn't (yet) a CI version matrix

The acceptance criteria for #645 also call for adding a version matrix to CI and fuzzing/compatibility test automation. That requires actually building and testing against each candidate SDK version in a CI job — a workflow-level change, not a documentation one. This file exists so that follow-up work (or a maintainer deciding to attempt a 20.x/22.x build) starts from a written record of what's known instead of from scratch.

## Upgrade checklist (for the next SDK bump)

When bumping the pinned version in the root `Cargo.toml`:

1. Run `cargo build --workspace` for every contract crate before touching any contract logic — a bump that doesn't even compile tells you immediately whether the SDK made a breaking API change.
2. Check the SDK's release notes for any change to how `#[contracttype]` enums are serialized when used as storage keys directly (several contracts in this workspace do this, and rely on stable, collision-free encoding — see the discriminant-collision bug class fixed in `contracts/withdrawal/src/lib.rs`).
3. Re-verify every `env.invoke_contract` cross-contract call site still matches the callee's actual exported function name and signature — the SDK does not catch a mismatch here at compile time.
4. Update this file's version matrix with the result (supported / needs changes / broken), even if the upgrade doesn't proceed, so the next person doesn't repeat the investigation.
