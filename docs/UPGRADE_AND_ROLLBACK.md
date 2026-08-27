# Contract Upgrade and Rollback Procedure

This document describes the safe process for upgrading Soroban contracts and rolling back if issues are detected after deployment.

> **Closes #595** — Backward compatibility strategy for upgrades with state migration functions and upgrade safety checks.

Version numbers, compatibility rules, and `get_version` querying are defined in [VERSIONING.md](./VERSIONING.md). Run upgrades inside a [maintenance window](./MAINTENANCE_WINDOWS.md).

## Upgrade Safety Architecture

All Lumora contracts use `shared::upgrade` helpers to enforce a safe upgrade lifecycle:

- **`ContractVersion`** — semantic version struct persisted in instance storage so the current version is always readable on-chain.
- **`require_paused_for_upgrade`** — panics if the contract is not paused, ensuring no in-flight transactions run against a partially migrated state.
- **`record_upgrade`** — writes the new version and emits an `upgraded` event after WASM replacement.
- **`signal_migration_needed`** — emits a `mig_need` event indicating that an off-chain migration script must run before operations resume.

## State Migration Functions

Each contract exposes a versioned migration entry point callable by the admin after a schema-changing upgrade:

```rust
// Example: contract v1 → v2 migration entry point
pub fn migrate_v1_to_v2(env: Env, admin: Address) -> Result<(), ContractError> {
    admin.require_auth();
    // 1. Read all v1 records
    // 2. Transform to v2 schema
    // 3. Write v2 records
    // 4. Call shared::upgrade::record_upgrade(...)
    Ok(())
}
```

For schema-compatible upgrades (adding optional fields only), no migration function is needed — existing records remain valid and new fields default to `None` or `0`.

## Backward Compatibility Rules

1. **Never remove or rename a `#[contracttype]` variant** — doing so breaks ABI compatibility with existing stored records.
2. **New fields must be `Option<T>`** — deserialisation of old records will produce `None` for fields that did not exist at write time.
3. **Error code values are permanent** — never renumber a `#[contracterror]` variant; add new variants at the end only.
4. **Storage key values are permanent** — never change the discriminant of a `DataKey` enum variant after deployment.
5. **Deploy to a new contract ID** — always deploy the upgraded WASM to a fresh contract address and migrate traffic, rather than upgrading in-place (Soroban `update_current_contract_wasm` replaces the WASM but preserves storage).

## Prerequisites

- Admin secret key for the contract (admin must be set during initialization).
- Soroban CLI configured for the target network.
- New WASM binary compiled and tested.
- Current contract ID.

## Pre-Upgrade Validation

1. **Verify the new WASM**:
   ```bash
   soroban contract inspect --wasm target/wasm32-unknown-unknown/release/new_contract.wasm
   ```

2. **Compare storage keys**: Ensure the new contract version does not change existing storage key formats unless a migration is explicitly written.

3. **Review events**: Confirm no existing events are removed or have their payloads changed.

4. **Run the full test suite**:
   ```bash
   cargo test --workspace
   ```

5. **Check WASM size**:
   ```bash
   ls -lh target/wasm32-unknown-unknown/release/new_contract.wasm
   ```

## Upgrade Procedure

1. Pause the contract (see [PAUSE_AND_EMERGENCY.md](./PAUSE_AND_EMERGENCY.md)):
   ```bash
   soroban contract invoke --id <CONTRACT_ID> --network <NETWORK> --source <ADMIN> -- pause
   ```

2. Deploy the new WASM to a fresh contract ID:
   ```bash
   NEW_ID=$(soroban contract deploy \
     --wasm target/wasm32-unknown-unknown/release/new_contract.wasm \
     --network <NETWORK> --source <ADMIN>)
   ```

3. (Schema-changing upgrades only) Run the migration entry point:
   ```bash
   soroban contract invoke --id $NEW_ID --network <NETWORK> --source <ADMIN> -- migrate_v1_to_v2
   ```

4. Verify record counts via view functions:
   ```bash
   soroban contract invoke --id $NEW_ID -- get_version
   ```

5. Reinitialize the new contract with the existing admin and configuration.

6. Run smoke tests against the new contract.

7. Redirect traffic to the new contract ID.

8. Unpause the new contract and verify normal operations:
   ```bash
   soroban contract invoke --id $NEW_ID --network <NETWORK> --source <ADMIN> -- unpause
   ```

## Rollback Criteria

Rollback is triggered if any of the following are detected within the monitoring window:

- Token transfers fail or produce incorrect amounts.
- Event payloads are malformed or missing.
- Contract panics with unexpected errors.
- Storage contains corrupted or missing data.

## Rollback Procedure

1. **Immediate**: Call `pause` on the new contract to halt operations.
2. **Restore**: Point all traffic back to the old contract ID.
3. **Verify**: Run the verification steps against the old contract.
4. **Investigate**: Fix the issue in the new WASM and repeat the upgrade process.

## Post-Upgrade Monitoring

Monitor the following for at least 24 hours after upgrade:

- Transaction success rate (target: >99%).
- Event emission completeness.
- Storage record consistency (use view functions).
- Error rate per operation type.
- `upgraded` event visible in the contract event stream.

## Rollback Safety Considerations

- Upgrades are one-directional on Soroban: the old WASM is replaced in-place.
- Always deploy to a new contract ID first and migrate traffic, rather than upgrading in place.
- Keep the old contract ID and deployment artifacts for at least 30 days.
- Test the rollback procedure on testnet before using it on mainnet.
