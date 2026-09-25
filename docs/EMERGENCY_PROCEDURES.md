# Emergency Pause and Recovery Runbook

> Closes **#707** — emergency pause and recovery mechanism.
>
> Related: [PAUSE_AND_EMERGENCY.md](./PAUSE_AND_EMERGENCY.md) (mechanism
> reference), [MAINTENANCE_WINDOWS.md](./MAINTENANCE_WINDOWS.md) (pause order,
> backups, templates), [OPERATIONAL_RUNBOOK.md](./OPERATIONAL_RUNBOOK.md),
> [DEPLOYMENT.md](./DEPLOYMENT.md), [COMMUNICATION_TEMPLATES.md](./COMMUNICATION_TEMPLATES.md).

---

## 0. Scope: what this change did and did not do

This is important, so read it before you rely on anything below.

**The pause mechanism already exists and is NOT part of this change.**

`contracts/shared/src/pause.rs` provides the whole of the current mechanism:

| Item | Present today |
|------|---------------|
| `PauseDataKey::Paused` | yes |
| `require_not_paused(env)` | yes |
| `pause(env, admin)` / `unpause(env, admin)` | yes |
| `ContractPausedEvent` / `ContractUnpausedEvent` | yes |
| Consumer contracts | `escrow`, `campaign`, `donation`, `withdrawal`, `revenue_sharing` |

`escrow` additionally keeps its **own** pause key (`PauseKey::Paused`,
`PauseKey::Admin`), a typed `EscrowError::ContractPaused = 14` surfaced as
`symbol_short!("PAUSED")`, and a public `is_paused` view. The escrow pause flag
is independent of the shared one; pausing one does not pause the other.

`unpause` is **immediate**. There is no time-lock, no multi-signature, and no
delayed recovery anywhere in the workspace today.

**What this change (this PR) contains:**

- This runbook — the operator-facing procedures and escalation path.
- Pause/resume tests in `tests/framework/tests/pause_recovery.rs`.

**What this change does NOT contain, and why:**

The time-locked recovery mechanism is a duplicate of issue **#711** and is
being implemented in a **companion pull request**. That PR owns
`contracts/shared/src/pause.rs` and adds the recovery delay, a public
`is_paused` on the shared module, and a typed error. This PR deliberately does
not touch that file, so the two can be reviewed and merged independently
without a conflict. §5 below describes the recovery delay as a *specification
for the companion PR*, not as something you can use today.

---

## 1. Who is authorised

| Role | Can pause / resume | Notes |
|------|--------------------|-------|
| Contract admin (the address stored at `initialize`) | yes | The only on-chain-authorised identity |
| Any other address | **no** | `pause` compares against the stored admin and returns `Unauthorized` |
| Anyone with the admin *key* | yes | Keys are held in a secret manager, never in this repo |

There is no multi-signature today. **If the admin key is lost, there is no
on-chain path to pause or resume.** That is a single point of failure and is the
main motivation for #711. Until it lands, the mitigation is operational: two
operators, an escrowed offline backup of the admin key, and a rehearsed
rotation.

Which address is the admin differs per contract:

- `escrow` — the address passed to `escrow.initialize(admin)`.
- Contracts using `shared::pause` — the address each entry point passes as its
  `admin` argument. Read it from the config contract
  (`platform_config.get_config().admin`) for the contracts that resolve it that
  way.

Read the admin off-chain before an incident, not during one.

---

## 2. Detecting that a contract is paused

In order of cost, cheapest first.

### 2.1 A call that should work fails

The most common signal. `shared::pause` panics with the literal string
`contract is paused`; `escrow` returns `EscrowError::ContractPaused` (code 14).
Neither is a ledger-level failure, so a retry loop will not clear it.

### 2.2 The event stream

A pause always emits, so the absence of the event plus a failure means the
contract was never paused and the problem is elsewhere.

| Contract | Pause topics | Unpause topics |
|----------|--------------|----------------|
| `escrow` (own pause key) | `("esc", "paused")` | `("esc", "unpaused")` |
| `shared::pause` consumers | `("contract_paused",)` | `("contract_unpaused",)` |

### 2.3 An explicit read — `escrow` only

```bash
soroban contract invoke --id "$ESCROW_ID" --network "$NETWORK" -- is_paused
```

`false` means running. **`scripts/deploy/verify_deploy.sh` runs exactly this
check** and fails if escrow is found paused after a deploy.

### 2.4 A read for the shared flag — not available yet

`shared::pause` has **no public `is_paused`**. The `Paused` key lives in
instance storage, so today the only way to read it is to attempt a guarded
call and observe the failure, or to read the contract's instance entry
directly with a state-reading tool. Issue #711 adds a public `is_paused`; this
runbook will point at it once it lands. Until then, use §2.1 and §2.2.

---

## 3. Pausing

### 3.1 Order

Pause **dependents before dependencies**, so a paused callee is never invoked by
a still-live caller. `MAINTENANCE_WINDOWS.md §2` has the full ordered list:

1. `donation`, `withdrawal`
2. `campaign`
3. `escrow`, `commission_agreement`, `subscription`, `revenue_sharing`, `creator_fund`
4. `dispute_arbiter`, `messaging`, `competitions`, `verification`, `recruitment`
5. `platform_config` — **last**; other contracts may still need to read fees
   while draining.

### 3.2 The commands

```bash
# Requires the admin key in the environment, never an argument.
export STELLAR_DEPLOYER_SECRET='...'   # injected from a secret manager
NETWORK=testnet                        # or mainnet

# Contracts that use the shared pause module, when the entry point takes an
# admin argument:
soroban contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$STELLAR_DEPLOYER_IDENTITY" \
  -- \
  pause --admin "$ADMIN_ADDRESS"

# Escrow, which keeps its own pause key:
soroban contract invoke \
  --id "$ESCROW_ID" \
  --network "$NETWORK" \
  --source "$STELLAR_DEPLOYER_IDENTITY" \
  -- \
  pause --admin "$ESCROW_ADMIN_ADDRESS"
```

`shared::pause(env, admin)` requires auth from the `admin` **argument**, so the
`--admin` value must be the address that will sign. `escrow.pause` does the
same and additionally rejects any address that is not the stored `PauseKey::Admin`.

### 3.3 Confirm the pause took effect

Do all three. A `pause` that returned success but did not block anything is
worse than no pause, because it produces false confidence.

1. `is_paused` returns `true` (`escrow` only — §2.3).
2. A guarded non-admin call now fails with `contract is paused` /
   `ContractPaused`. Pick the cheapest read-only-ish operation the contract
   guards; do **not** probe with a real money movement.
3. The `("esc", "paused")` or `("contract_paused",)` event is in the stream.

---

## 4. Resuming

### 4.1 Before you resume

Resuming a contract that is still vulnerable re-exposes users. Do not resume
because an incident is "probably over" — resume because the fix is deployed and
verified, or because the pause was itself the bug.

- [ ] Root cause identified and written down.
- [ ] Fix deployed, or the false alarm explained.
- [ ] On a **new** contract id if the fix changes code, or in place for a
      PATCH/MINOR per [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md).
- [ ] Smoke test passed on a testnet clone.
- [ ] Users notified (templates in [COMMUNICATION_TEMPLATES.md](./COMMUNICATION_TEMPLATES.md)).
- [ ] Ledger headroom checked: a long pause can let TTL expire on records.
      Escrows use `ESCROW_TTL_LEDGERS` (~30 days) and disputed escrows use the
      dispute TTL; a pause longer than that window can destroy escrow records
      you still need.

### 4.2 The commands

```bash
soroban contract invoke \
  --id "$CONTRACT_ID" --network "$NETWORK" \
  --source "$STELLAR_DEPLOYER_IDENTITY" -- \
  unpause --admin "$ADMIN_ADDRESS"
```

`unpause` is **immediate** — there is no delay to wait out. (See §5 for the
delay the companion PR adds.)

### 4.3 Order

Reverse of §3.1: `platform_config` first, then the marketplace contracts, then
`campaign`, then `donation`/`withdrawal`, then `escrow` and the rest. **Run one
smoke transaction after each contract**, not after all of them — that is how you
find out which contract was the one still broken.

### 4.4 Verifying state after resume

For each contract, in this order:

1. The unpause succeeded and the pause event is absent from the new call.
2. `escrow`: `is_paused` returns `false`.
3. A previously-blocked operation now succeeds. On `escrow` that is
   `create_escrow` or `refund_client` — the two entry points the pause guard
   covers. Do this on **testnet with a throwaway record**, never on mainnet
   with a real one.
4. Records created *before* the pause are still readable. A pause must not
   change state, so a pre-pause record must return the same value it did
   before. If it does not, you are looking at TTL expiry (§4.1), not at the
   pause.
5. `health_check` and `get_health_metrics` look healthy, and no
   auto-rollback has fired (`should_rollback()` is `false`).
6. Error rate back to baseline before moving to the next contract.

---

## 5. Time-locked recovery — specification only, not available yet

**The mechanism described in this section does not exist in the workspace at the
time of writing.** It is the subject of a companion PR for issue **#711**,
which owns `contracts/shared/src/pause.rs`. This runbook documents the
procedure the operator will follow once it does, so the runbook is ready on the
day it ships. Do not write a runbook entry that promises a delay the code does
not enforce.

### What #711 adds, as specified

- A **recovery delay**: after `unpause` is requested, the contract stays
  effectively paused until a configurable number of ledgers has elapsed.
  Pausing itself stays immediate — you never want to slow down a stop.
- A **public `is_paused`** on the shared module, so §2.4 above is a cheap read
  rather than a probe.
- A **typed error** for a resume attempted before the delay has elapsed,
  distinct from `contract is paused`, so a client can tell "you are early" from
  "this is broken".

### The procedure it implies

1. The incident is resolved and the fix is verified on a clone.
2. A responder calls `unpause` (or `schedule_unpause`, depending on the final
   API). The contract stays in recovery until the delay expires. The
   transaction that requested the resume and the ledger at which it will take
   effect are both recorded in the incident log.
3. During the delay, the delay is visible to anyone reading the contract, and
   the responder can still **re-pause** — the recovery is not a one-way door.
4. After the delay elapses, the first read confirms the state flipped.
5. Smoke test (§4.4) then runs in full.

### What the delay is for

A single compromised admin key can otherwise pause *and* resume inside one
transaction window, undoing a stop before anyone notices. A delay turns a
one-transaction attack into something that is visible for a full window. The
delay is only meaningful if the pause and the resume are announced publicly —
so the escalation in §6 exists to make sure they are.

**The delay length is a governance decision, not an engineering one.** Before
#711 ships it needs: a chosen ledger count with its wall-clock equivalent at
the expected ledger close time, an agreement that the delay applies to
*emergency* resumes as well as planned ones, and an answer for "what if the
incident is still open when the delay expires" (answer: re-pause, and expect to
do it again).

---

## 6. Escalation path

| Stage | Trigger | Who | Action |
|-------|---------|-----|--------|
| 0 — Detection | Any §2 signal | On-call monitoring | Open an incident. Record the contract id, the network, the ledger, and the exact failure. |
| 1 — Containment | Confirmed funds at risk, or an active exploit | On-call operator | Pause the affected contract and its dependents (§3). Notify within **15 minutes** (`MAINTENANCE_WINDOWS.md §1`). |
| 2 — Second operator | Any stage-1 pause on **mainnet** | Second operator joins | Confirms the pause. No mainnet pause is single-operator. |
| 3 — Assessment | After stage 1 | Incident lead + second operator | Root cause. Decide: fix forward, roll back, or leave paused. |
| 4 — Communication | After stage 1 on mainnet | Incident lead | Status page + Discord + email. Templates in [COMMUNICATION_TEMPLATES.md](./COMMUNICATION_TEMPLATES.md). |
| 5 — Recovery | Root cause fixed and verified | Incident lead, both operators | Deploy, smoke test on a clone, then resume (§4). |
| 6 — Review | Within 72 hours of resolution | Whole team | Post-incident review (§7). |

If the admin key is unavailable at stage 1, escalate immediately and treat it as
a key-rotation incident: the contracts cannot be stopped. There is no multi-sig
fallback today (§1, and §5 for the direction).

---

## 7. Post-incident checklist

- [ ] All affected contracts resumed, or deliberately left paused with a written
      reason.
- [ ] `is_paused` is `false` everywhere it can be read (§2.3).
- [ ] One full happy path exercised per resumed contract, on testnet.
- [ ] `health_check` healthy on every in-scope contract; `should_rollback()`
      `false`.
- [ ] Pre-pause records spot-checked for TTL survival (§4.1).
- [ ] Error rate back to baseline, and held there for the monitoring window in
      [SLA.md](./SLA.md).
- [ ] Rollout state sane — if a canary auto-rolled back, decide deliberately
      whether to re-promote it ([DEPLOY.md §Canary](./DEPLOY.md)).
- [ ] Timeline written: detection, each pause, the fix, the resume, each
      notification, with ledger numbers.
- [ ] The deployed WASM hash for every version involved recorded.
- [ ] Blameless post-incident review scheduled within 72 hours.
- [ ] Follow-ups filed. If any of them is "pause should have been multi-sig" or
      "there was no way to tell we were paused", that is #711 — link it.

---

## 8. What is tested, and what is not

`tests/framework/tests/pause_recovery.rs` covers, against the `escrow`
contract in the in-process Soroban test environment:

1. A guarded operation is blocked while paused, and the error is the typed
   `EscrowError::ContractPaused` rather than a bare panic.
2. The same operation succeeds again after `unpause`, and the record it creates
   is correct.
3. A non-admin address cannot pause, and cannot unpause.
4. `("esc", "paused")` and `("esc", "unpaused")` are both emitted, and each is
   emitted only by its own action.

Those four are the behaviours this runbook depends on. What is **not** covered
by any test, and cannot be:

- The `shared::pause` module's behaviour end to end, from a consuming
  contract's entry point. The shared module is exercised indirectly, but no
  test in this PR drives it through `campaign`, `donation`, `withdrawal` or
  `revenue_sharing`.
- Anything about the time-locked recovery in §5. It does not exist yet.
- Anything about real ledger timing, TTL expiry under a long pause, or a
  multi-sig policy. Those need a network, not a test environment.
