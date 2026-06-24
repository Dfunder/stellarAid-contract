# Security Checklist — StellarAid Factory Contract

> Reviewed against commit implementing issues #272, #273, #274.  
> Scope: `factory/src/lib.rs` and `campaign/src/` contracts.

---

## 1. Authorization on All Mutations

| Function | Auth Check | Result | Notes |
|---|---|---|---|
| `initialize` | `admin.require_auth()` | ✅ Pass | One-shot; re-init guarded by `has(&DataKey::Admin)` |
| `update_wasm_hash` | `require_admin()` → `admin.require_auth()` | ✅ Pass | Admin-only |
| `set_deployment_fee` | `require_admin()` → `admin.require_auth()` | ✅ Pass | Admin-only |
| `deploy_campaign` | `creator.require_auth()` | ✅ Pass | Creator must sign their own deployment |
| `campaign::donate` | `donor.require_auth()` | ✅ Pass | Donor signs token transfer |
| `campaign::release_milestone` | `creator.require_auth()` | ✅ Pass | Only campaign owner may release |
| `campaign::freeze` / `unfreeze` | `admin.require_auth()` | ✅ Pass | Admin-only freeze controls |
| `campaign::upgrade` | `admin.require_auth()` | ✅ Pass | Admin-only WASM upgrade |
| `campaign::set_admin` | `admin.require_auth()` | ✅ Pass | Admin-only rotation |

**Overall: ✅ Pass** — every state-mutating entry point requires an explicit `require_auth()` call.

---

## 2. Reentrancy

Soroban's execution model is single-threaded and does not allow cross-contract callbacks to re-enter the same contract instance during a transaction.  All state writes in `deploy_campaign` happen **after** the token transfer and deploy calls (check-effects-interactions is followed naturally).

| Check | Result | Notes |
|---|---|---|
| No recursive cross-contract call pattern | ✅ Pass | Soroban host prevents reentrancy at VM level |
| State updates (registry, counter) occur after external calls | ✅ Pass | Counter/registry updated post-deploy |
| Fee transfer uses `token::Client` (Soroban-native call) | ✅ Pass | Atomic within the transaction; no callback surface |

**Overall: ✅ Pass** — reentrancy is not possible in Soroban, and the code follows check-effects-interactions regardless.

---

## 3. Integer Overflow

| Location | Type | Check | Result | Notes |
|---|---|---|---|---|
| `DataKey::Count` increment | `u64` | Arithmetic `count + 1` | ✅ Pass | Soroban SDK panics on overflow in debug; in release, wrapping would reset counter — acceptable at 2^64 deployments |
| `deployment_fee` storage | `i128` | No arithmetic performed on-chain | ✅ Pass | Fee is only passed to `token::transfer`; the token contract enforces balance checks |
| Campaign donation amounts | `i128` | SDK token arithmetic | ✅ Pass | Soroban token standard handles overflow |

**Overall: ✅ Pass** — no custom integer arithmetic that could overflow in practice.

---

## 4. Access Control

| Control | Mechanism | Result | Notes |
|---|---|---|---|
| Single admin model | `DataKey::Admin` set at init, checked via `require_admin()` | ✅ Pass | Consistent pattern across all admin functions |
| Admin rotation | `campaign::set_admin` requires current admin auth | ✅ Pass | No privilege escalation path |
| Factory admin ≠ campaign admin | Factory and campaign admins are independent addresses | ✅ Pass | Appropriate separation of concerns |
| Treasury address immutability | Set at `initialize`, no setter exposed | ✅ Pass | Treasury cannot be redirected after deploy; consider adding `set_treasury` with admin auth if needed |
| WASM hash update | Only admin can call `update_wasm_hash` | ✅ Pass | Old campaigns unaffected; only future deploys use new hash |
| Deployment fee update | Only admin can call `set_deployment_fee` | ✅ Pass | Creator cannot bypass fee |

**Overall: ✅ Pass** — access control is correctly scoped and consistently enforced.

---

## 5. Event Completeness

| Action | Event Published | Topics | Data | Result | Notes |
|---|---|---|---|---|---|
| `deploy_campaign` | `campaign_deployed` | `(symbol, creator)` | `deployed_address` | ✅ Pass | Sufficient for backend indexing |
| `update_wasm_hash` | None | — | — | ⚠️ Warn | Consider emitting `wasm_hash_updated` for auditability |
| `set_deployment_fee` | None | — | — | ⚠️ Warn | Consider emitting `deployment_fee_updated` |
| `initialize` | None | — | — | ✅ Pass | Init events are optional; one-shot events have limited value |
| `campaign::donate` | `donated` | `(symbol, campaign)` | `(donor, amount, asset)` | ✅ Pass | Full donor info captured |
| `campaign::release_milestone` | `milestone_released` | `(symbol, campaign)` | `(index, amount)` | ✅ Pass | Sufficient for audit trail |
| `campaign::freeze` | `frozen` | `(symbol, campaign)` | `admin` | ✅ Pass | |
| `campaign::unfreeze` | `unfrozen` | `(symbol, campaign)` | `admin` | ✅ Pass | |

**Overall: ✅ Pass (with minor warnings)** — core events are present and indexed. Two admin mutation events are missing; tracked below.

---

## Fail / Warning Items → Tracked Issues

| # | Severity | Description | Recommended Action |
|---|---|---|---|
| W1 | Low | `update_wasm_hash` emits no event | Open issue: "Emit `wasm_hash_updated` event in factory" |
| W2 | Low | `set_deployment_fee` emits no event | Open issue: "Emit `fee_updated` event in factory" |
| W3 | Low | `DataKey::Treasury` has no setter; treasury address is permanently fixed | Open issue: "Add `set_treasury` (admin-only) to factory" |

> All three items are low severity and do not represent exploitable vulnerabilities.  
> No **Fail** items were identified during this review.
