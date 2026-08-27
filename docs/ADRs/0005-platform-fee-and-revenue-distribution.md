# [ADR-0005] Dynamic Platform Fee Governance and Multi-Party Splits

* **Status:** Accepted
* **Date:** 2026-03-10
* **Authors:** StellarAid Tokenomics Team
* **Deciders:** Foundation Governance
* **Component:** Platform Config (`contracts/platform_config`) & Revenue Sharing (`contracts/revenue_sharing`)

---

## Context and Problem Statement

Platform fees fund protocol maintenance and sustainability. Fees must be dynamic (adjustable via two-step admin governance), subject to strict protocol caps (maximum 1,000 bps = 10%), and queryable via standardized helper contracts with safe fallbacks. Furthermore, revenue-sharing agreements require multi-party basis-point distributions with rounding dust protection.

## Decision Drivers

* Hardcoded maximum fee safety invariant (`fee_bps <= 1000`).
* Two-step admin transfer (`transfer_admin` -> `accept_admin`) preventing accidental loss of ownership.
* Resilient cross-contract config lookups (`try_get_fee_bps` with default fallbacks).
* Deterministic integer math ensuring exact balance conservation.

## Decision Outcome

**Chosen Option:** Dedicated `PlatformConfigContract` holding global protocol parameters, paired with `shared::config` validation wrappers.

### Positive Consequences

* Any contract can query fees and platform wallet addresses safely.
* Fee updates emit `fee_bps_updated` events for real-time auditability.
* Two-step admin transition eliminates single-point admin transfer risks.

---

## Implementation & Code References

* Contract: `contracts/platform_config/src/lib.rs`
* Helpers: `contracts/shared/src/config.rs`
* Revenue Sharing: `contracts/revenue_sharing/src/lib.rs`
