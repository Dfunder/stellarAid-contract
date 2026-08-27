# [ADR-0004] Autonomous Dispute Arbiter & Cross-Contract Resolution

* **Status:** Accepted
* **Date:** 2026-03-01
* **Authors:** StellarAid Governance & Architecture Team
* **Deciders:** Core Engineering Team
* **Component:** Dispute Arbiter (`contracts/dispute_arbiter`)

---

## Context and Problem Statement

When disagreements arise between clients and service providers, an escrow enters a `Disputed` state. Resolution requires a trusted arbitrator (admin, DAO, or multi-sig) capable of executing full payouts to the client, full payouts to the artist, or customized partial percentage splits (in basis points), with a fallback auto-resolution timelock if the arbitrator fails to act within `auto_resolve_ledgers`.

## Decision Drivers

* Separation of concerns: Escrow holds assets; Arbiter enforces dispute logic.
* Time-bounded disputes preventing indefinite fund freeze.
* Granular basis point splits (`client_share_bps + artist_share_bps == 10000`).
* Cross-contract invocation with least-privilege guarantees.

## Considered Options

1. **Option 1: Independent Dispute Arbiter Contract with Cross-Contract Escrow Calls** — Dedicated contract authenticated by Escrow contract.
2. **Option 2: Embedding Arbitration Directly Inside Escrow Contract** — Monolithic contract.
3. **Option 3: Off-Chain Signed Attestations (Oracle)** — Oracle signatures submitted directly to Escrow.

## Decision Outcome

**Chosen Option:** **Option 1: Dedicated Dispute Arbiter Contract**.

The separation decouples dispute business logic, auto-resolve countdowns, and governance mechanisms from the underlying low-level escrow custody contract.

### Positive Consequences

* Arbiter rules can be upgraded independently of escrow custody code.
* Automatic fallback: If unresolved after `auto_resolve_ledger`, anyone can trigger `auto_resolve()` to refund the client.
* Standardized cross-contract client interface (`escrow_client.rs`).

---

## Implementation & Code References

* Contract: `contracts/dispute_arbiter/src/lib.rs`
* Client: `contracts/dispute_arbiter/src/escrow_client.rs`
* Types & Errors: `contracts/dispute_arbiter/src/types.rs`, `contracts/dispute_arbiter/src/errors.rs`
