# [ADR-0006] Structured Event Emission for Off-Chain Indexing

* **Status:** Accepted
* **Date:** 2026-03-25
* **Authors:** StellarAid Data & Indexing Team
* **Deciders:** Core Engineering Team
* **Component:** All Contracts / Off-chain Indexer

---

## Context and Problem Statement

Decentralized applications, client SDKs, and mobile wallets require real-time notifications and relational indexing of on-chain state transitions without relying on expensive RPC contract storage queries.

## Decision Drivers

* Uniform event topic naming convention across all 13 contracts.
* Compact, typed payloads minimizing WASM footprint and gas fees.
* Strict ordering guarantees for off-chain ingestion pipelines.

## Decision Outcome

Adopt a standard **two-element topic tuple convention**:
```rust
env.events().publish(
    (symbol_short!("<domain>"), symbol_short!("<action>")),
    (payload_field_1, payload_field_2, ...),
);
```

### Event Topics Matrix

| Domain | Action | Meaning |
|--------|--------|---------|
| `escrow` | `created` | Escrow created & locked |
| `escrow` | `released` | Escrow payout released |
| `escrow` | `refunded` | Escrow refunded |
| `escrow` | `disputed` | Dispute initiated |
| `agr` | `created` | Commission agreement initialized |
| `ms` | `approved` | Milestone approved |
| `config` | `fee_upd` | Platform fee updated |

### Positive Consequences

* Off-chain indexers can filter by single or compound topic wildcards.
* Enables event stream replay and reconciliation against ledger snapshots.

---

## Implementation & Code References

* Specification: `docs/EVENTS.md`
* Event schemas: `docs/event_schemas.md`
