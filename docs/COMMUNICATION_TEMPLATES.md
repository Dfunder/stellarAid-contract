# Communication Templates

> Closes **#681** — copy-paste notices for scheduled and emergency contract maintenance.

Do **not** include secret keys, unreleased WASM hashes that embed secrets, or individual user balances. Post to the status page, Discord `#status`, and email `ops@` / `users@` as applicable.

Run these inside a [maintenance window](./MAINTENANCE_WINDOWS.md).

## Scheduled maintenance (T–72h / T–48h / T–24h)

**Subject:** Lumora scheduled maintenance — [DATE] [START]–[END] UTC

```
We will perform planned maintenance on Stellar [testnet|mainnet] contracts.

When: [DAY], [DATE] [START]–[END] UTC ([local conversion])
Contracts: [list names + IDs]
Impact: [donations / escrow create / withdrawals] paused for up to [N] minutes
What you should do: avoid submitting [donate|create_escrow|…] during the window
Version: upgrading [name] from [old semver] to [new semver] (see CHANGELOG)
Status: https://[status-page]
Contact: [on-call rotation / Discord]
```

## Window start

**Subject:** Lumora maintenance started — [DATE]

```
Maintenance has started at [HH:MM] UTC.
Contracts [list] are paused. Do not submit state-changing transactions.
We will post again when operations resume, or at [HH:MM] UTC if the window is extended.
```

## Emergency pause

**Subject:** [EMERGENCY] Lumora contracts paused

```
We paused [contract list] at [HH:MM] UTC after detecting [one-line symptom, no exploit details].
Funds in existing escrows/campaigns remain in contract storage; token accounts are not frozen.
Do not send further [donations|escrow deposits] until we unpause.
Next update by [HH:MM] UTC ([max 30 min]).
Incident lead: [name]
```

## Upgrade complete

**Subject:** Lumora maintenance complete — [contract] [new semver]

```
Maintenance finished at [HH:MM] UTC.
[contract] is live at version [new semver] (storage schema [n]).
Contract ID: [unchanged | new ID …]
Please update SDKs / config if the ID changed.
Hypercare: we will watch success rate for 24 hours. Report issues to [channel].
```

## Rollback

**Subject:** Lumora rollback — traffic restored to [old semver]

```
The [new semver] deployment did not pass smoke checks. Traffic is back on
[old contract ID] at [old semver]. The new ID is paused and will not receive funds.
User action: none if you use our hosted API; self-hosted indexers should pin [old ID].
We will share a follow-up after the post-incident review.
```

## Window cancelled / postponed

```
The [DATE] UTC maintenance window for [contracts] is cancelled / moved to [new DATE].
Reason: [weathered incident / failed testnet rehearsal / …].
No pause will occur on the original date.
```

## Internal war-room checklist

```
- [ ] Window type: scheduled / emergency
- [ ] Backup path: backups/[timestamp]
- [ ] Pause order completed at:
- [ ] Versions recorded (get_version_metadata)
- [ ] Upgrade / migrate tx hashes:
- [ ] Unpause order completed at:
- [ ] Templates sent: T-72 / start / complete / rollback
- [ ] Hypercare owner (24h):
```
