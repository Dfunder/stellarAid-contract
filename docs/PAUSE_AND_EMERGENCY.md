# Pause and Emergency Stop Workflow

This document describes how operators can pause and unpause contracts, and the escalation path for emergency situations.

## Overview

Contracts in this workspace implement a standard pause mechanism via the shared `pause` module (`contracts/shared/src/pause.rs`). When a contract is paused, all non-admin operations are blocked with a `"contract is paused"` panic.

Scheduled and emergency pauses belong inside a **maintenance window**. See [MAINTENANCE_WINDOWS.md](./MAINTENANCE_WINDOWS.md) for pause order across all contracts, backup steps, and user-facing templates.

## Prerequisites

- Admin address for the contract (set during initialization via PlatformConfig).
- Soroban CLI configured for the target network.
- Contract ID of the contract to pause.

## Pausing a Contract

1. Ensure the admin account has sufficient XLM for transaction fees.
2. Call the `pause` function on the target contract:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  pause
```

3. Verify the contract is paused by attempting a non-admin operation — it should fail with `"contract is paused"`.

## Unpausing a Contract

1. Use the admin account to call:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  unpause
```

2. Verify normal operations resume.

## Risks and Considerations

- Pausing does **not** halt token transfers — it only blocks contract entry points.
- Active escrows, agreements, and disputes remain in their current state.
- Long-duration pausing may cause ledger-based expiry to trigger for escrow records.
- Always notify affected users before pausing in production.

## Emergency Stop (Planned)

For severe security incidents, an emergency stop extends the pause mechanism with:

- **Multi-sig requirement**: requires approval from 2 of N designated addresses.
- **Timelock**: pausing is immediate; emergency unpause has a 24-hour delay.
- **Events**: `emergency_pause_initiated`, `emergency_pause_executed`, `emergency_unpause_scheduled`.

This mechanism is not yet implemented. Track issue #TBD for progress.

## Post-Action Validation

After pausing or unpausing:

1. Confirm the contract state via `get_config` or equivalent view function.
2. Run the integration test suite against the paused/unpaused contract.
3. Monitor event logs for `contract_paused` / `contract_unpaused` events.
