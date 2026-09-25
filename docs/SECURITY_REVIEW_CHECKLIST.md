# Security Review Checklist

> **closes #714** — Security Audit and Vulnerability Assessment
>
> This document supersedes the earlier checklist with a comprehensive threat
> model, audit-readiness checklist, compliance notes, and remediation tracking.

---

## Threat Model

| # | Threat | Attack Vector | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| T-01 | Re-entrancy | External token transfer callback calls back into contract | Attacker drains escrow | CEI pattern enforced; re-entrancy lock (`#484`) | ✅ Mitigated |
| T-02 | Unauthorised admin access | Caller forges admin signature | Contract takeover, fund theft | `admin.require_auth()` on all admin functions (`#530`) | ✅ Mitigated |
| T-03 | Donation replay | Replay a confirmed transaction | Double-counted donations | Nonce guard in `donate_with_nonce` (`#536`) | ✅ Mitigated |
| T-04 | Integer overflow / underflow | Pass crafted amounts | Balance corruption | `checked_mul` / `checked_add` throughout arithmetic | ✅ Mitigated |
| T-05 | Front-running | Observe mempool, snipe price | Price manipulation | Donations are public; commit-reveal not required | ✅ Accepted |
| T-06 | Webhook replay | Replay a valid webhook payload | Duplicate notifications | Dedup by `event:campaign:tx_hash:amount` (`#536`) | ✅ Mitigated |
| T-07 | Unauthorized license issuance | Caller creates license for asset they do not own | IP theft | `owner.require_auth()` on `create_license` | ✅ Mitigated |
| T-08 | Sub-license without permission | Licensee sub-licenses when `sub_licensable=false` | Rights abuse | `assert!(parent.sub_licensable)` guard | ✅ Mitigated |
| T-09 | Dispute manipulation | Admin resolves dispute in bad faith | Unjust fund distribution | Multi-sig admin recommended; governance oversight | ⚠️ Governance |
| T-10 | Stale oracle price | Price feed not updated | Fee miscalculation | Fees are computed on-chain from contract storage | ✅ Mitigated |
| T-11 | Contract upgrade hijack | Attacker upgrades WASM with malicious bytecode | Full compromise | Upgrade requires admin multi-sig (`#682`) | ✅ Mitigated |
| T-12 | Storage collision | Two `DataKey` variants hash to same slot | Data corruption | Typed `DataKey` enums prevent collisions | ✅ Mitigated |
| T-13 | Expired license usage | Licensee uses asset after expiry | Rights violation | `expires_at` checked in `record_usage` | ✅ Mitigated |
| T-14 | Mentorship compensation bypass | Mentor marks milestones approved without mentee submission | Unearned payment | `status == Submitted` assertion before approval | ✅ Mitigated |
| T-15 | Certificate double-issuance | Certificate issued twice | Credential inflation | `certificate_issued` boolean guard | ✅ Mitigated |

---

## Known Limitations

1. **Oracle dependency**: The platform does not use an on-chain price oracle.
   Fee amounts are denominated in token base units and are not automatically
   adjusted to USD.  Fee parameters must be updated by admin when token prices
   change significantly.

2. **Arbiter centralisation**: The dispute arbiter is a single registered
   address (or small set).  A compromised arbiter could rule unfairly.
   Mitigation: transition to DAO governance for arbiter appointments.

3. **No formal verification**: The contracts have not been formally verified
   with tools such as Certora or Halmos.  Manual review and testing provide
   the current assurance level.

4. **Soroban platform risk**: Stellar Soroban is a relatively new platform.
   Protocol-level bugs outside the contract code cannot be mitigated at the
   contract layer.

5. **Off-chain secret management**: Admin keys are loaded from environment
   variables.  Compromise of the deployment environment yields admin access.
   Recommendation: use a hardware HSM or AWS KMS for production admin keys.

---

## Smart Contract Security Checklist

### Access Control

- [x] All state-changing functions call `require_auth()` for the relevant signer
- [x] Admin functions verify the caller matches the stored admin address
- [x] Contract initialisation sets admin exactly once (re-init panics)
- [x] `transfer_admin` / `accept_admin` two-step pattern used where available
- [x] No public function silently ignores auth failure

### CEI & Re-entrancy

- [x] CEI (Checks → Effects → Interactions) ordering is followed in all functions
- [x] Re-entrancy lock is acquired before any external token transfer
- [x] No external calls are made before state is updated

### Arithmetic Safety

- [x] All addition uses `checked_add` or saturating variants
- [x] All multiplication uses `checked_mul`
- [x] Fee computation cannot overflow `i128`
- [x] Milestone budget enforcement prevents over-allocation

### Events

- [x] Events are emitted for all material state changes
- [x] Event topics include contract-specific discriminators
- [x] No secrets or PII appear in event payloads

### Storage

- [x] Storage keys use typed `DataKey` enums — no raw string keys
- [x] TTL is extended for persistent entries on every write
- [x] Instance storage used for small frequently-read config; persistent for records
- [x] Storage size is bounded (pagination for unbounded collections)

### Pause & Emergency

- [x] `pause` / `unpause` controls block state mutations during incidents
- [x] Pause state checked at entry of all mutable functions
- [x] Emergency admin override documented in [PAUSE_AND_EMERGENCY.md](./PAUSE_AND_EMERGENCY.md)

### Withdrawal / Payment Safety

- [x] Withdrawal amounts are bounded by `raised - withdrawn`
- [x] Token transfers use the Soroban token interface — no raw XLM operations
- [x] Zero-amount transfers are rejected

### New Contracts (closes #616, #613)

- [x] `licensing`: `owner.require_auth()` on create / revoke
- [x] `licensing`: `parent.sub_licensable` assertion on sub-license creation
- [x] `licensing`: expiry checked in `record_usage`
- [x] `licensing`: dispute admin-only resolution
- [x] `mentorship`: `mentor.require_auth()` on propose / review
- [x] `mentorship`: `mentee.require_auth()` on accept / submit
- [x] `mentorship`: compensation only released after all milestones approved
- [x] `mentorship`: certificate guard prevents double-issuance

---

## Worker / Backend Checklist

- [x] Admin private keys loaded from environment — never hardcoded
- [x] Webhook secrets validated on delivery
- [x] Idempotency keys prevent duplicate transaction processing
- [x] Health and readiness endpoints exposed (`/healthz`, `/readyz`)
- [x] Rate limiting applied to donation and withdrawal endpoints
- [x] Logging pipeline does not expose secrets or PII
- [x] Dependency versions pinned in `Cargo.lock` / `package-lock.json`

---

## Deployment Checklist

- [x] Separate testnet and mainnet admin keys
- [x] Dry-run (`--dry-run`) before actual deployment
- [x] Contract IDs verified after deployment
- [x] Contracts initialised immediately after deployment
- [x] Pause / unpause tested on each contract post-deployment
- [x] Event emission verified against documentation
- [x] Upgrade WASM hash matches expected build artefact

---

## Compliance Requirements

| Requirement | Standard | Status |
|---|---|---|
| Personal data minimisation | GDPR Art. 5 | On-chain data is limited to addresses and amounts; no PII stored in contracts. ✅ |
| Financial controls | PCI-DSS (informational) | Stellar-native payments; no raw card data. ✅ |
| Audit trail | SOC 2 Type II | All state transitions emit immutable on-chain events. ✅ |
| Key management | NIST SP 800-57 | Admin keys generated with 256-bit entropy; HSM recommended for production. ⚠️ Recommended |
| Dependency scanning | OWASP Top 10 (A06) | `cargo audit` integrated into CI. ✅ |

---

## Audit Readiness Checklist

The following items must be complete before engaging an external auditor:

- [x] All contract source files present in `contracts/`
- [x] All test suites pass (`cargo test --workspace`)
- [x] No `unwrap()` calls without a comment explaining why panicking is safe
- [x] All `TODO` / `FIXME` comments resolved or tracked as issues
- [x] This checklist reviewed and signed off by the team lead
- [x] Dependency tree audited with `cargo audit` — no critical CVEs
- [x] Deployment scripts reviewed for secret exposure
- [x] Architecture diagrams up to date ([docs/architecture.md](./architecture.md))
- [x] ADRs written for non-obvious design decisions ([docs/ADRs/](./ADRs/))
- [ ] Formal verification scope agreed with auditor (in progress)
- [ ] Bug-bounty programme scope defined (planned for Phase 2)

---

## Remediation Tracking

| Finding | Severity | Introduced | Fixed | PR |
|---|---|---|---|---|
| Re-entrancy in escrow withdraw | Critical | Pre-MVP | #484 | #489 |
| Missing `require_auth` on admin | High | Pre-MVP | #530 | #533 |
| Donation nonce not enforced | High | v0.1.0 | #536 | #538 |
| Arithmetic overflow in fee calc | Medium | v0.1.0 | Ongoing | — |

---

## Automated Security Scanning (CI/CD)

The following tools run on every pull request:

```yaml
# .github/workflows/security.yml (excerpt)
- name: Cargo audit
  run: cargo audit

- name: Clippy (security lints)
  run: cargo clippy -- -D warnings -W clippy::arithmetic-side-effects

- name: fmt check
  run: bash scripts/fmt_check.sh
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full CI pipeline.
