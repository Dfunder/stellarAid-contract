# StellarAid Contract Architecture

> **Summary.** This document gives the high-level architecture of the stellarAid
> contract workspace: which crates exist, which of them call which, and how a
> request flows from a client through the contracts to an emitted event. The
> workspace has grown well past the three-contract sketch this file originally
> carried — there are now 29 member directories, of which 24 are contracts or
> the shared library. See [`CONTRACTS.md`](./CONTRACTS.md) for the per-contract
> reference and [`STORAGE.md`](./STORAGE.md) for storage layouts.

## 1. Overview

The platform is a set of Soroban smart contracts deployed on Stellar. Each
contract owns its own storage; they compose by **cross-contract calls** rather
than by sharing a database. A `shared` crate provides reusable machinery
(pagination, pause, health, circuit breaker, feature-flag rollout, typed
errors) that is inlined into each calling contract's WASM, so it acts on the
**caller's** storage namespaces.

The architecture has three layers:

1. **Configuration layering.** `platform_config` holds the values the money
   contracts share: fee basis points, accepted token ids, feature flags and
   pause/rollout state. Money contracts read it through cross-contract calls
   rather than duplicating the values.
2. **Money movement.** `escrow`, `commission_agreement`, `dispute_arbiter`,
   `donation`, `withdrawal`, `revenue_sharing`, `creator_fund`, `subscription`
   and `campaign` hold or move funds. They are the contracts for which pause,
   rollback and TTL policy matter most.
3. **Derived and auxiliary services.** `analytics`, `audit`, `reputation`,
   `verification`, `search`, `messaging`, `nft`, `dao`, `licensing`,
   `mentorship`, `competitions`, `recruitment`, `rate_limiter`, `multi_sig`,
   `ecosystem_funding` and the campaign factories provide records and views
   derived from the money contracts' activity.

## 2. Diagram

The diagram below shows the original commission flow (agreement → dispute →
event emission). It is retained as the visual version; the prose above and the
contract table in §3 carry the same information in text so the structure is
available without reading the box-drawing characters.

```text
    +---------------------------+
    | CommissionAgreement       |
    |---------------------------|
    | create_agreement()        |
    | sign_agreement()          |
    | release_commission()      |
    +---------------------------+
               |
               v
    +---------------------------+
    | DisputeArbiter            |
    |---------------------------|
    | raise_dispute()           |
    | resolve_dispute()         |
    +---------------------------+
               |
               v
    +---------------------------+
    | Event Emitter             |
    |---------------------------|
    | AgreementCreated          |
    | DisputeResolved           |
    +---------------------------+
```

**Reading the diagram in prose:** a client and an artist create a commission
agreement in `commission_agreement`, which both parties then sign. When the work
is released, `release_commission()` pays the artist and records the platform
fee. If either party disputes the agreement, the case is handed to
`dispute_arbiter`, whose `resolve_dispute()` decides the outcome. Every one of
these state transitions emits an event, so an indexer sees the whole flow from
the event stream even though no single contract stores the end-to-end history.

## 3. Contract inventory

Every entry below is a workspace member. "Deployed" reflects the honest status
in [`CONTRACTS.md`](./CONTRACTS.md): crates with known pre-existing compilation
defects are marked as such rather than described as live.

| Crate | Role | Calls / called by |
|---|---|---|
| `shared` | Library of reusable machinery (pagination, pause, health, circuit breaker, rollout, errors). No entry points. | Inlined into every other contract. |
| `platform_config` | Shared configuration: fee bps, token ids, feature flags, pause/rollout state. | Read by the money contracts. |
| `escrow` | Holds funds for a job and releases them on milestone completion. | Reads `platform_config`. |
| `commission_agreement` | Commission terms, milestones and fee release. | Reads `platform_config`; disputes go to `dispute_arbiter`. |
| `dispute_arbiter` | Raises and resolves disputes over escrows and agreements. | Called by `escrow` / `commission_agreement` clients. |
| `donation` | Per-campaign donation records and totals. | Reads `platform_config`. |
| `withdrawal` | Pending/settled withdrawal records against a campaign. | Reads `platform_config`. |
| `campaign` | Campaign lifecycle and funding. | Reads `platform_config`. |
| `revenue_sharing` | Splits recorded revenue across recipients. | Calls the token contract. |
| `creator_fund` | Allocations and growth of the creator fund. | Reads `platform_config`. |
| `subscription` | Tiered subscriptions and credit ledgers. | Calls the token contract. |
| `analytics` | Earnings ledger and lifetime metrics. | Written by payout paths. |
| `audit` | Append-only audit trail of sensitive operations. | Written by other contracts. |
| `reputation` | Reviews, ratings and a moderation queue. | Standalone. |
| `verification` | Artist verification records. | Standalone. |
| `search` | Denormalised listing index and search. | Standalone. |
| `messaging` | Per-conversation message history. | Standalone. |
| `nft` | NFT ownership and royalties. | Calls the token contract. |
| `dao` | Governance proposals and voting. | Standalone. |
| `licensing` | Creative licence records. | Standalone. |
| `mentorship` | Mentorship programmes and milestones. | Calls the token contract. |
| `competitions` | Competition lifecycle, entries and prize distribution. | Calls the token contract. |
| `recruitment` | Job postings and applications. | Standalone. |
| `rate_limiter` | Configurable per-key rate limits. | Used by other contracts. |
| `multi_sig` | Multi-signature proposals and approvals. | Standalone. |
| `ecosystem_funding` | Grant programmes and milestone review. | Calls the token contract. |
| `campaign_factory` / `campaign_instance` / `factory` | Deploy and register campaign instances. | Deploy `campaign`. |

## 4. Request flow

A single client action crosses contracts as follows:

1. The client submits a transaction to a contract entry point (for example
   `escrow::create_escrow`).
2. The entry point calls `require_not_paused` (or the circuit-breaker guard) and
   `require_auth` on the acting address.
3. If the operation depends on shared configuration, the contract makes a
   cross-contract call to `platform_config` for the fee bps or token id.
4. If funds move, the contract calls the Stellar Asset Contract to transfer.
5. State is written under the contract's own `DataKey` namespace with the
   appropriate TTL extension, and one or more events are published.
6. Off-chain indexers read the event stream; the `analytics` and `audit`
   contracts record derived history.

## 5. Cross-cutting concerns

- **Pausing.** `shared::pause` provides pause / unpause and a time-locked
  recovery. Not every contract currently exposes it; see
  [`PAUSE_AND_EMERGENCY.md`](./PAUSE_AND_EMERGENCY.md).
- **Health and circuit breaking.** `shared::health` and
  `shared::circuit_breaker` track error counters and can auto-halt.
- **Feature flags.** `shared::rollout` stores a bounded flag set (in the
  caller's instance storage) and a rollback path.
- **Storage and TTL.** Every contract follows the storage and TTL policy in
  [`STORAGE.md`](./STORAGE.md); retention is governed by
  [`RETENTION_AND_PRUNING.md`](./RETENTION_AND_PRUNING.md).
