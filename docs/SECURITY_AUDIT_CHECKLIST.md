# Comprehensive Security Audit Checklist

> **closes #627** — Add Comprehensive Security Audit Checklist
>
> This document provides a third-party-review-ready security audit checklist
> covering threat modelling, security assumptions, known limitations, and
> compliance requirements for all Lumora smart contracts.

---

## 1. Security Assumptions

The following assumptions are relied upon by the contract system.  A violation
of any assumption may invalidate one or more mitigations.

| ID | Assumption |
|---|---|
| SA-01 | The Stellar / Soroban network correctly enforces transaction signatures and does not permit signature forgery. |
| SA-02 | The admin private key is stored securely (HSM or equivalent) and is not compromised. |
| SA-03 | The governance token contract correctly tracks balances and cannot be double-counted. |
| SA-04 | Off-chain metadata URIs (IPFS CIDs) are content-addressed and tamper-evident. |
| SA-05 | The payment token contracts (USDC, XLM) behave according to the Stellar SEP-0041 token interface. |
| SA-06 | The Soroban host environment correctly isolates contract invocations (no cross-contract storage leakage). |
| SA-07 | Ledger sequence numbers monotonically increase and cannot be manipulated by the contracts themselves. |
| SA-08 | The multi-sig signers are trusted, independent parties who will not collude to execute a malicious proposal. |

---

## 2. Known Limitations

| ID | Limitation | Risk Level | Mitigation Path |
|---|---|---|---|
| KL-01 | No on-chain price oracle — fees are denominated in token base units. | Low | Acceptable; admin updates fee config as needed. |
| KL-02 | Single admin key — compromise yields full contract control. | High | Migrate to multi-sig admin or DAO governance in Phase 2. |
| KL-03 | No formal verification (Certora / Halmos). | Medium | Manual audit + extensive unit tests. |
| KL-04 | Soroban platform is relatively new; unknown protocol-level bugs may exist. | Medium | Follow Stellar Foundation security advisories. |
| KL-05 | Off-chain delivery proof (IPFS URIs) is not validated on-chain. | Low | Managers are responsible for verifying deliverables off-chain. |
| KL-06 | DAO governance execution is not atomic — admin must separately call target contract. | Medium | Accepted; reduces attack surface of generic call-data execution. |
| KL-07 | NFT royalties require buyer to have pre-approved the token contract allowance. | Low | Documented in SDK examples; client-side validation recommended. |
| KL-08 | Storage TTL extension does not guarantee indefinite persistence post-archival. | Medium | Monitor ledger archival windows; implement TTL bump workers. |

---

## 3. Threat Model

### 3.1 Attack Surface

```
External Users  →  Soroban RPC  →  Contract Entry Points
                                         │
                                    Storage Layer
                                         │
                              Token Contracts (SEP-0041)
                                         │
                              Worker / Backend Services
```

### 3.2 Threat Enumeration

| ID | Threat | Component | STRIDE | Likelihood | Impact | Risk | Mitigation |
|---|---|---|---|---|---|---|---|
| T-01 | Re-entrancy via token callback | escrow | Elevation | Low | Critical | High | CEI pattern; re-entrancy lock |
| T-02 | Unauthorised admin escalation | all | Spoofing | Low | Critical | High | `require_auth()` on all admin calls |
| T-03 | Donation / payment replay | escrow, campaign | Repudiation | Medium | High | High | Nonce guard; idempotency keys |
| T-04 | Integer overflow in fee calc | platform_config | Tampering | Low | Medium | Medium | `checked_*` arithmetic |
| T-05 | Front-running of escrow creation | escrow | Info Disclosure | Low | Low | Low | Accepted (public chain) |
| T-06 | Webhook replay attacks | worker | Repudiation | Medium | Medium | Medium | HMAC validation + dedup |
| T-07 | Unauthorised license issuance | licensing | Spoofing | Low | High | High | `owner.require_auth()` |
| T-08 | Sub-license without permission | licensing | Elevation | Low | Medium | Medium | `sub_licensable` assertion |
| T-09 | Dispute manipulation by admin | licensing, escrow | Tampering | Low | High | Medium | Governance oversight; event audit trail |
| T-10 | WASM upgrade hijack | all | Tampering | Very Low | Critical | High | Admin multi-sig on upgrade |
| T-11 | Storage key collision | all | Tampering | Very Low | High | Medium | Typed `DataKey` enums |
| T-12 | Governance vote manipulation | dao | Tampering | Medium | High | High | Token-weighted voting; quorum check |
| T-13 | Fake governance token | dao | Spoofing | Very Low | Critical | High | Token contract set at init; immutable |
| T-14 | NFT double-mint for same asset | nft | Repudiation | Low | Medium | Medium | Unique id counter; creator must sign |
| T-15 | Mentorship compensation without milestone | mentorship | Elevation | Low | Medium | Medium | All milestones must be `Approved` |
| T-16 | Ecosystem fund draining via milestone loop | ecosystem_funding | Elevation | Low | High | High | `disbursed += amount` bounded by `total_allocation` |
| T-17 | Expired license used for commercial gain | licensing | Repudiation | Medium | Medium | Medium | `expires_at` check in `record_usage` |
| T-18 | Stale TTL causing data loss | all | Denial of Service | Low | Medium | Medium | TTL bump worker monitors entries |

---

## 4. Smart Contract Security Checklist

### 4.1 Access Control

- [x] Every state-changing function calls `require_auth()` on the relevant party
- [x] Admin-only functions assert caller == stored admin
- [x] Contract initialisation is a one-time operation (re-init panics)
- [x] `transfer_admin` / `accept_admin` two-step pattern used where applicable
- [x] No public function silently swallows an auth failure
- [x] Multi-sig threshold enforced before execution in DAO contract
- [x] License owner is the only address that can revoke or update royalties

### 4.2 CEI & Re-entrancy

- [x] Checks performed before any state mutation
- [x] State updated before any external token transfer (CEI)
- [x] Re-entrancy lock on escrow withdrawal paths
- [x] No external calls made before local state is finalised

### 4.3 Arithmetic Safety

- [x] Addition: `checked_add` / `i128::checked_add`
- [x] Multiplication: `checked_mul` / `i128::checked_mul`
- [x] Fee basis-points capped at 10 000
- [x] Royalty basis-points capped at 3 000
- [x] Disbursement amounts validated as positive before storage
- [x] `total_disbursed += amount` cannot exceed `total_allocation` by contract invariant

### 4.4 Events

- [x] All material state changes emit a `publish` event
- [x] Event topics include contract-specific symbols
- [x] No private keys, secrets, or PII in event payloads

### 4.5 Storage

- [x] Typed `DataKey` enums prevent key collisions
- [x] TTL extended on every persistent write
- [x] Unbounded collections are paginated (usage entries, history, outcomes)
- [x] Instance storage used for small frequently-accessed config values

### 4.6 Pause & Emergency Controls

- [x] `pause` / `unpause` blocks mutations in escrow and platform_config
- [x] Pause state checked at the top of mutable functions
- [x] Emergency procedure documented in `docs/PAUSE_AND_EMERGENCY.md`

### 4.7 New Contracts (NFT, DAO, EcosystemFunding)

- [x] **NFT**: `creator.require_auth()` on mint / royalty update
- [x] **NFT**: `owner.require_auth()` on transfer / burn
- [x] **NFT**: Royalty capped at 30 %
- [x] **NFT**: Transfer history bounded and paginated
- [x] **NFT**: Freeze function restricts transfers without burning
- [x] **DAO**: Token balance checked before vote weight assignment
- [x] **DAO**: Double-vote prevention via `HasVoted` storage key
- [x] **DAO**: Quorum check before passing a proposal
- [x] **DAO**: Timelock enforced before execution
- [x] **DAO**: Multi-sig threshold enforced for sensitive proposals
- [x] **DAO**: Governance history recorded for every state transition
- [x] **EcosystemFunding**: `recipient.require_auth()` on milestone submission
- [x] **EcosystemFunding**: Disbursement only on approved milestones
- [x] **EcosystemFunding**: Outcomes paginated and bounded

---

## 5. Worker & Backend Checklist

- [x] Admin private keys loaded from environment variables — never hardcoded
- [x] Webhook secrets validated using HMAC on every delivery
- [x] Idempotency keys (dedup by `event:tx_hash:amount`) prevent duplicate processing
- [x] Health endpoint `/healthz` and readiness endpoint `/readyz` exposed
- [x] Rate limiting applied to donation, withdrawal, and proposal submission endpoints
- [x] Logging pipeline sanitised — no secrets or PII in log output
- [x] All dependency versions pinned (`Cargo.lock`, `package-lock.json`)

---

## 6. Deployment Security Checklist

- [x] Separate testnet and mainnet admin keypairs generated with 256-bit entropy
- [x] Dry-run (`--dry-run`) performed before any live deployment
- [x] Contract IDs verified against expected values after deployment
- [x] All contracts initialised immediately after deployment
- [x] Pause / unpause tested on every contract post-deployment
- [x] Event emission verified against `docs/EVENTS.md` schemas
- [x] Upgrade WASM hash matches expected build artefact SHA-256
- [x] Admin key rotation procedure documented and tested

---

## 7. Compliance Requirements

| Requirement | Standard | Notes | Status |
|---|---|---|---|
| Personal data minimisation | GDPR Art. 5 | On-chain data limited to addresses and amounts; no PII. | ✅ |
| Right to erasure | GDPR Art. 17 | Blockchain data is immutable; users are informed at onboarding. | ℹ️ Disclosed |
| Financial controls | PCI-DSS (informational) | Stellar-native payments; no raw card data. | ✅ |
| Audit trail | SOC 2 Type II | Immutable on-chain event log for all state transitions. | ✅ |
| Key management | NIST SP 800-57 | Admin keys 256-bit; HSM recommended for mainnet. | ⚠️ HSM Recommended |
| Dependency scanning | OWASP A06 | `cargo audit` integrated in CI; no critical CVEs in current tree. | ✅ |
| Secure coding | OWASP A03 | Input validation via Rust type system and explicit asserts. | ✅ |
| Supply chain security | SLSA Level 1 | Builds are reproducible; pinned dependencies. | ✅ |

---

## 8. Audit Readiness Checklist

Items that must be complete before engaging an external security auditor:

- [x] All contract source files present under `contracts/`
- [x] All workspace tests pass (`cargo test --workspace`)
- [x] No unresolved `unwrap()` calls without documented justification
- [x] All `TODO` / `FIXME` comments resolved or tracked as GitHub issues
- [x] This checklist reviewed and signed off by team lead
- [x] `cargo audit` run — no critical CVEs in dependency tree
- [x] Deployment scripts reviewed for secret exposure
- [x] Architecture diagrams up to date (`docs/architecture.md`)
- [x] ADRs written for all non-obvious design decisions (`docs/ADRs/`)
- [x] Full API documentation available (`docs/COMPREHENSIVE_API_DOCS.md`)
- [ ] Formal verification scope agreed with auditor *(in progress)*
- [ ] Bug-bounty programme scope defined *(planned — Phase 2)*
- [ ] External audit report integrated into this document *(pending engagement)*

---

## 9. Automated Security Scanning

The following tools are integrated into the CI pipeline and run on every PR:

```yaml
# Excerpt from CI configuration
- name: cargo audit
  run: cargo audit

- name: clippy security lints
  run: cargo clippy --workspace -- -D warnings -W clippy::arithmetic-side-effects -W clippy::integer-arithmetic

- name: format check
  run: bash scripts/fmt_check.sh

- name: dependency license check
  run: cargo deny check licenses
```

---

*Document maintained by the Lumora security team.  Last reviewed: 2026-09-25.*
