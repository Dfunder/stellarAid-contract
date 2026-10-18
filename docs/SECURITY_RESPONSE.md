# Security Response Procedure

> **Closes #1011.** This is the procedure for a **reported security
> vulnerability**, from "report received" to "fix deployed and users informed".
>
> It does **not** replace the operational runbooks — it starts where they end
> and links to them:
>
> - [`SECURITY.md`](../SECURITY.md) — how a researcher reports a vulnerability,
>   and the response expectations promised to them. **KEEP THE TWO IN SYNC:** do
>   not promise a response time here that `SECURITY.md` does not promise, or
>   vice versa. (`SECURITY.md` is added by #1009; this document assumes it.)
> - [`EMERGENCY_PROCEDURES.md`](./EMERGENCY_PROCEDURES.md) — pausing, resuming,
>   the time-locked recovery, escalation and the post-incident checklist. This
>   procedure **calls into** that runbook; it does not repeat it.
> - [`OPERATIONAL_RUNBOOK.md`](./OPERATIONAL_RUNBOOK.md) — worker health,
>   incident response, recovery procedures.
> - [`PAUSE_AND_EMERGENCY.md`](./PAUSE_AND_EMERGENCY.md) — the pause entry
>   points per contract.
> - [`MAINTENANCE_WINDOWS.md`](./MAINTENANCE_WINDOWS.md) — take a state backup
>   before any pause or upgrade.
> - [`DEPLOYMENT.md`](./DEPLOYMENT.md) and [`UPGRADE_AND_ROLLBACK.md`](./UPGRADE_AND_ROLLBACK.md)
>   — the deploy, verify and rollback mechanics referenced below.
>
> **Security checklists.** The audit and review checklists
> ([`SECURITY_AUDIT_CHECKLIST.md`](./SECURITY_AUDIT_CHECKLIST.md),
> [`SECURITY_REVIEW_CHECKLIST.md`](./SECURITY_REVIEW_CHECKLIST.md)) overlap and
> are being consolidated under #834. Use **one** consolidated checklist for the
> review gate in §4; do not run both. Until #834 lands, treat
> `SECURITY_REVIEW_CHECKLIST.md` as the reviewing document and
> `SECURITY_AUDIT_CHECKLIST.md` as the reference it cites.

---

## Roles (by responsibility, not by name)

A single-maintainer project still benefits from naming the decisions. One person
may hold several roles, but each decision below has exactly one owner.

| Role | Owns the decision |
|---|---|
| **Triage Lead** | Severity, exploitability, funds at risk, affected contracts and versions. Decides whether to escalate. |
| **Incident Commander (IC)** | Runs containment. Authorises a pause / rollback. Is the single point of decision under time pressure. |
| **Release Engineer** | Builds, verifies and deploys the fix. Owns the WASM upgrade and `get_version` check. |
| **Comms Lead** | Tells integrators and users. Writes the advisory. Owns the researcher's channel. |

If only one person is available, that person is the IC and records who made
each decision, even when that is themselves.

---

## 1. Triage (receive → decide severity)

**Input:** a report (email, `SECURITY.md` channel, or a private advisory).
**Output:** a severity and an escalate/don't decision, recorded.

1. Acknowledge the report. Do not promise a timeline beyond what `SECURITY.md`
   states.
2. Reproduce or disprove the finding against `main`. Record the **commit**.
3. Classify:

   | Question | Why it drives the response |
   |---|---|
   | Is it exploitable against a **deployed** contract? | A finding in a library-only path is a normal bug fix, not an incident. |
   | Does it risk **funds**? | If yes, §2 containment is immediate; a funds-risk finding does not wait for a fix. |
   | Which contracts and **versions**? | Determines what must be paused and what must be redeployed. |
   | Is the exploit **public**? | Public ⇒ disclosure clock starts now (§5). |

4. Record the classification in the incident tracker. If the finding is not
   applicable (dependency path never compiled, feature never enabled), close it
   with the reason — do not leave it open as "maybe".

---

## 2. Containment (decide, pause, back up)

**Input:** a triage decision of "funds at risk" or "live exploit".
**Output:** the affected contracts are stopped or the decision not to stop is
recorded.

1. **Decide which contracts to pause.** Follow
   [`EMERGENCY_PROCEDURES.md` §3](./EMERGENCY_PROCEDURES.md) for the order and
   the exact commands. Pause the money-path contracts the finding touches first.
2. **Authorise.** The IC authorises the pause; the operator executes it. Record
   the ledger and the transactions.
3. **Back up first.** Take a state backup before pausing, per
   [`MAINTENANCE_WINDOWS.md` §3](./MAINTENANCE_WINDOWS.md):

   ```sh
   ./scripts/backup_contract_state.sh
   ```

   A pause longer than the escrow TTL is a **data-retention** event, not just an
   availability event — see [`RETENTION_AND_PRUNING.md` §5](./RETENTION_AND_PRUNING.md).
4. **If a rollback is needed**, use the existing surface (available on the
   contracts that expose it — see the gap below) rather than a bespoke
   operation. Record it like any pause.

### Containment gap — state this honestly

- **Pause is not uniform.** Only **4 of 29** contracts currently expose `pause`,
  and only **1** exposes the circuit breaker. Containment is therefore not
  possible on every contract today. If the finding is in a contract that cannot
  be paused, containment is limited to:
  - pausing the contracts that **call** it, or that it calls, if they can be
    paused; and
  - an off-chain advisory telling users to stop transacting.
  Closing this gap (`Add pause and unpause entry points to the 25 contracts that
  lack them`, #955) is a prerequisite for a uniform procedure.
- **Rollback** is exposed on **16 of 29** contracts; the rest cannot be rolled
  back and must be patched forward.

---

## 3. Communication

**Input:** a contained incident.
**Output:** integrators and users know whether to transact.

1. **Integrators first.** The four sibling repositories and any known
   integrators are told before a public advisory, because they can act. Use the
   templates in [`COMMUNICATION_TEMPLATES.md`](./COMMUNICATION_TEMPLATES.md).
2. **Say the actionable thing:** which contracts are stopped, whether funds are
   at risk, and whether users should stop transacting. Do **not** publish the
   exploit before a fix ships unless the exploit is already public.
3. **One channel, one author.** The Comms Lead owns outbound messaging; the
   researcher's channel stays with the Comms Lead so the researcher is updated,
   not surprised.
4. Keep the researcher informed at the cadence promised in `SECURITY.md`.

---

## 4. Fix, review and deploy

**Input:** a fix ready for review.
**Output:** a patched, verified deployment.

1. **Fix on a private branch.** A security fix is a normal PR from the repo's
   point of view, but it is not disclosed until §5.
2. **Review against one consolidated checklist** (§ preamble; #834). The
   security CI (`security.yml`) must pass, or each failure must be an explicitly
   reviewed waiver in `deny.toml` / `.cargo/audit.toml`.
3. **A patch is a WASM upgrade**, so verify it like one:

   ```sh
   ./scripts/deploy/deploy.sh          # or make deploy-*
   ./scripts/verify_deployment.sh
   make validate
   ```

4. **Verify `get_version` after deploy.** A security fix must be confirmed to
   have actually landed.

   > **Known divergence.** 17 of 20 contracts currently report the
   > **compile-time** version rather than the deployed one. `get_version` is
   > therefore **not** a reliable confirmation of what is deployed for those
   > contracts today. Until the `get_version` divergence is fixed, confirm the
   > deployment by the upgrade transaction and the WASM hash recorded by
   > `./scripts/verify_deployment.sh`, and treat `get_version` as advisory.
5. Record the deployed WASM hash and the transaction in the incident tracker.

---

## 5. Disclosure

**Input:** a deployed fix, or a decision to disclose without one.
**Output:** a public advisory and a closed report.

1. **Embargo.** Default to a coordinated embargo until the fix is deployed
   across affected contracts. The length must match what `SECURITY.md` promises;
   do not invent a different number here.
2. **Advisory or CVE.** Decide and record who decides:
   - **Advisory** (GitHub Security Advisory) for a platform-specific issue.
   - **CVE** when the finding is likely to affect other integrations or third
     parties will reference it.
   The Comms Lead authors it; the Triage Lead confirms the technical content.
3. Publish only after affected contracts are patched, or after the embargo
   expires, whichever is later — unless the exploit is already public, in which
   case disclose the mitigation (not the exploit detail) immediately.
4. Credit the reporter as they requested.

---

## 6. Post-incident

Complete [`EMERGENCY_PROCEDURES.md` §7](./EMERGENCY_PROCEDURES.md) — resume,
verify, and backfill the incident review. In addition, for a vulnerability:

- [ ] The root cause is recorded, not just the fix.
- [ ] If the finding class applies elsewhere, a sweep for the same class across
      the other contracts is filed (the same defect often exists in siblings).
- [ ] Any containment that was **not** possible (a contract with no `pause`, or `get_version`
      that could not confirm the deploy) is recorded as a gap with an owner.
- [ ] `SECURITY.md` and this procedure still agree on the promised response
      times and channels.

---

## Appendix — concrete entry points

| Step | Command / entry point |
|---|---|
| Report channel | [`SECURITY.md`](../SECURITY.md) (#1009) |
| Back up state | `./scripts/backup_contract_state.sh` |
| Pause / resume | `pause` / `unpause` per [`EMERGENCY_PROCEDURES.md` §3–4](./EMERGENCY_PROCEDURES.md); coverage gaps in §2 above |
| Roll back | the existing rollback surface (16 of 29 contracts) |
| Deploy a fix | `make deploy-all` / `./scripts/deploy/deploy.sh` |
| Verify a deployment | `./scripts/verify_deployment.sh`, `make validate` |
| Confirm the patch landed | `get_version` (advisory only — see §4) |
| Templates | [`COMMUNICATION_TEMPLATES.md`](./COMMUNICATION_TEMPLATES.md) |
