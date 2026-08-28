# [ADR-0007] Soroban Storage Tiering and TTL Extension Strategy

* **Status:** Accepted
* **Date:** 2026-04-05
* **Authors:** StellarAid Core Engineering
* **Deciders:** Architecture Working Group
* **Component:** Storage / Lifecycle Management

---

## Context and Problem Statement

Soroban employs state expiration where storage entries require rent and will be archived if their Time-To-Live (TTL) reaches zero. Contracts must classify data into appropriate storage tiers (Instance, Persistent, Temporary) and execute automatic TTL extensions on critical user records.

## Decision Drivers

* Prevent inadvertent archiving of active escrows, agreements, and admin configurations.
* Minimize rent fee burden on callers by extending TTL only during active interactions.
* Distinguish transient locks (Temporary storage) from persistent financial records.

## Storage Classification Strategy

| Tier | Usage | TTL Extension Policy | Examples |
|------|-------|----------------------|----------|
| **Instance Storage** | Contract-wide metadata, Admin, Config pointers | Auto-extended on every contract invocation | `DataKey::Admin`, `DataKey::ConfigContract` |
| **Persistent Storage** | Financial records, escrows, milestones, campaigns | Extended to `DEFAULT_DISPUTE_TTL_LEDGERS` (864,000 ledgers ~ 40 days) on access | `DataKey::Escrow(id)`, `DataKey::Agreement(id)` |
| **Temporary Storage** | Mutex locks, replay nonces, transient rate-limit buckets | Expired naturally at zero rent cost | `DataKey::MilestoneLock(id, ms_id)` |

## Decision Outcome

Implement explicit TTL threshold bumps using `env.storage().persistent().extend_ttl(...)` and `env.storage().instance().extend_ttl(...)` within storage accessors.

---

## Implementation & Code References

* Escrow storage: `contracts/escrow/src/storage.rs`
* Constants: `DEFAULT_DISPUTE_TTL_LEDGERS = 864_000`
* Storage documentation: `docs/STORAGE.md`
