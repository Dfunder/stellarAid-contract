# Upgrade Testing (closes #689)

How upgrades are tested for the StellarAid workspace before any release is cut.

## Why this crate exists

Deploying new WASM over a live contract is risky only because of *state*: the new
binary must keep reading everything the old binary wrote, and any new shapes (keys,
fields) must be derived from the old ones without loss. `tests/upgrade_testing`
models that contract lifecycle in the Soroban test environment, so a release cannot
break stored data silently.

## The environment

`tests/upgrade_testing` is a workspace member that depends only on the Soroban SDK
(no network, no RPC). It provides:

- **Mock contract pair** — `src/v1.rs` and `src/v2.rs` are `#[contract]` mocks with a
  shared, permanent key layout (`src/keys.rs`). v1 is the "currently deployed WASM";
  v2 is the "new WASM". v2 keeps the whole v1 surface and *adds* keys + a `migrate`
  entry point, exactly like a real contract upgrade.
- **Simulator** (`src/simulator.rs`) — `UpgradeSimulator` snapshots a contract's
  instance/persistent storage, applies a migration to the snapshot, and restores it
  into a fresh v2 deployment. This is how an in-place swap is modelled.
- **Regression suite** (`src/regression.rs`) — after an upgrade it re-probes every
  behaviour that must not regress and reports a boolean verdict.
- **Scenarios** (`src/scenarios.rs`) — the concrete test cases below.

### Storage copy rules enforced by the framework

- `#[contracttype]` variants and their values are permanent and never repurposed.
- Schema-compatible upgrades only *add* keys; never change existing layouts.
- New derived keys are written by a migration that must be idempotent.

## Upgrade types and their tests

| Upgrade type | Meaning | Scenario |
|---|---|---|
| Schema-compatible swap | New WASM, same keys/layouts | `schema_compatible_upgrade_preserves_state` |
| Legacy read compat | v1 records decode under v2 with no migration run | `backward_compat_keeps_old_records_readable` |
| State migration | `migrate` entry point derives new keys from old ones | `state_migration_populates_new_keys` |
| Simulated migration | Framework transform applied to the snapshot | `framework_transform_migrates_restored_state` |
| Regression | Full battery passes post-upgrade | `regression_suite_runs_after_upgrade` |
| Idempotency | Re-running migration is a successful no-op | `state_migration_populates_new_keys` |

## How to run

```console
cargo test -p upgrade_testing
```

Run with output to see each scenario name:

```console
cargo test -p upgrade_testing -- --nocapture
```

## Writing a scenario for a real contract upgrade

1. Define the *old* and *new* keys in the crate's `DataKey` (or document the real
   contract's `DataKey` fate in the scenario comment).
2. Seed v1 (using the old entry points) with records that exercise every key.
3. Choose the migration:
   - no key change → `migrate_SchemaCompatible`;
   - new derived keys → a transform like `migrate_RosterToShares`, or a follow-up
     `migrate` call on the upgraded instance.
4. Assert: old records still readable (backward compat), new keys populated,
   migration idempotent, `storage_schema` bumped, `version` matches the release.
5. Extend `regression_suite` whenever the contract's public surface grows.

## Linking to a release

- Bump a contract to `vN+1` and run `cargo test -p upgrade_testing` before merging.
- If any legacy shape changes, that is a **breaking** upgrade: it must be mirrored
  here as a schema-migrating scenario *and* announced in the release notes before it
  ships (see `docs/UPGRADE.md` and `docs/UPGRADE_AND_ROLLBACK.md`).