# [ADR-0003] Multi-Milestone Commission Agreements and Concurrency Locks

* **Status:** Accepted
* **Date:** 2026-02-15
* **Authors:** StellarAid Product & Architecture Team
* **Deciders:** Smart Contract Engineers
* **Component:** Commission Agreement (`contracts/commission_agreement`)

---

## Context and Problem Statement

Commission workflows often involve multi-stage deliverables with incremental milestones. The contract must allow clients and artists to create agreements, propose milestones against an agreed budget cap, approve deliverables, handle pro-rata cancellations, and prevent concurrent race conditions during milestone state updates.

## Decision Drivers

* Budget cap invariant: Sum of all milestone amounts must never exceed `budget_usdc`.
* Atomic milestone approval and automatic agreement completion when all milestones are approved.
* Mutex lock on milestone transitions to prevent simultaneous conflicting updates.
* Flexible cancellation policy (penalty basis points, grace periods, pro-rata payouts).

## Considered Options

1. **Option 1: Per-Agreement Vector with Transient Mutex Lock** — Store milestones in `DataKey::MilestonesForAgreement(Bytes)` and individual records in `DataKey::Milestone(Bytes, Bytes)` protected by `DataKey::MilestoneLock(Bytes, Bytes)`.
2. **Option 2: Monolithic Agreement Struct with Fixed-Size Milestone Array** — Single large struct updated on every call.
3. **Option 3: External Milestone Sub-contracts** — Deploying individual contracts per milestone.

## Decision Outcome

**Chosen Option:** **Option 1**.

This approach provides fast indexed lookups for single milestone approvals while maintaining an authoritative vector of milestone IDs for aggregate agreement status validation. Transient storage locks guarantee serial execution.

### Positive Consequences

* Granular gas usage when querying single milestones.
* Budget overrun is strictly guarded at insertion time.
* Clean cancellation settlement math honoring completed milestones.

---

## Implementation & Code References

* Contract: `contracts/commission_agreement/src/lib.rs`
* Errors: `contracts/commission_agreement/src/errors.rs` (`MilestoneLocked`, `MilestoneBudgetExceeded`)
* Storage: `DataKey::Agreement`, `DataKey::MilestonesForAgreement`, `DataKey::CancellationPolicy`
