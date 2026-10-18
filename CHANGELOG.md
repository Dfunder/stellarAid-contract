# Changelog

All notable changes to Lumora / StellarAid contracts are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
How versions are stored, queried, and constrained is defined in [docs/VERSIONING.md](docs/VERSIONING.md).

## [Unreleased]

### Added

- `reputation::get_score(subject)` and `get_aggregate(subject)`, a maintained per-subject reputation aggregate with all components exposed, so a client no longer recomputes a score itself. `search`'s `set_rating` is documented as a private ranking hint; this contract is the authoritative reputation source. (#974)
- `reputation::set_appeal_window` / `get_appeal_window` / `get_appeal_deadline` / `ledgers_to_appeal`, bounding how long an appeal may be filed (`DEFAULT_APPEAL_WINDOW_LEDGERS` 17,280, `MAX_APPEAL_WINDOW_LEDGERS` 518,400). `report_review` now enforces a bounded moderation queue (`MAX_QUEUE_SIZE` 256) and per-review report cap (`MAX_REPORTS_PER_REVIEW` 16). (#976)
- `multi_sig::expire_proposal`, plus `is_expired` and `ledgers_until_expiry`, so an elapsed proposal can be finalised as `Expired` without waiting for a signing attempt, and its remaining time is readable. New `MultiSigError::NotExpired` (code 15). (#975)
- `dispute_arbiter` evidence deadline: `get_dispute_deadline` / `ledgers_until_deadline` / `submit_evidence` / `get_evidence` / `get_evidence_count`, and `set_dispute_ttl_ledgers` / `get_dispute_ttl_ledgers`. `DisputeRecord.auto_resolve_ledger` is renamed `deadline_ledger` and is the single timer for both the evidence window and `auto_resolve`. New `DisputeError` variants `EvidenceWindowClosed` (11), `EvidenceTooLong` (12), `InvalidTtl` (13). (#977)
- `shared::dispute::{DEFAULT_DISPUTE_TTL_LEDGERS, MAX_DISPUTE_TTL_LEDGERS}`, the single cross-contract source both `escrow` and `dispute_arbiter` read, so their dispute timers cannot drift apart. (#977)

### Changed

- `reputation`, `multi_sig`, and `dispute_arbiter` bump `storage-schema` 1 → 2 for the encoding changes above; `docs/STORAGE.md` is updated for all three crates.
- **`donation` — bounded retention for donation lists (#986).** Both
  `DataKey::DonationHistory(donor)` and `DataKey::CampaignDonations(campaign_id)`
  are now ring-trimmed on every write to an admin-configurable cap
  (`DEFAULT_DONATION_HISTORY = 100`, code ceiling `MAX_DONATION_HISTORY = 500`),
  so a popular campaign can no longer grow an unbounded vector that every later
  read has to pay for. New entry points: `get_donation_retention_cap`,
  `get_donation_retention_policy`, `set_donation_retention_cap`, and the bounded
  `prune_campaign_donations`.
- **`withdrawal` — bounded per-campaign index (#986).** `WithdrawalsByCampaign`
  now stores `Vec<u64>` ids instead of full records — the canonical
  `Withdrawal(id)` is stored once, so records are no longer duplicated — and is
  ring-trimmed to an admin-configurable cap
  (`DEFAULT_WITHDRAWALS_PER_CAMPAIGN = 100`, ceiling
  `MAX_WITHDRAWALS_PER_CAMPAIGN = 500`). New: `get_withdrawal_retention_cap`,
  `get_withdrawal_retention_policy`, `set_withdrawal_retention_cap`, and
  `prune_withdrawals_by_campaign`.
- **`search` — bounded, indexed scan (#987).** Listings are denormalised into
  `DataKey::Index(u32)` entries (`ArtistIndexEntry`) with an `IndexCount` and a
  reverse `ArtistIndex(Address)` map. `search` now scans at most
  `MAX_SCAN = 200` index entries and reads only the returned page's `Listing`
  records; `SearchResultPage` gained `scanned` and `truncated` so a partial
  result is explicit rather than silently incomplete.
- **`escrow` — `AtomicCommit` TTL and bounded lifetime (#988).** The atomic
  commit marker is now TTL-extended on every access
  (`storage::ATOMIC_COMMIT_TTL_LEDGERS = 432_000`), and an `InProgress` marker
  older than `cross_contract::ATOMIC_COMMIT_MAX_LIFETIME_LEDGERS = 120_960` is
  resolved to the terminal `RolledBack` state, so a commit can no longer be
  stranded in limbo or expire mid-flight. `get_atomic_commit` routes through the
  refreshing reader.
- **`revenue_sharing` — `record_revenue` benchmark (#989).** A new
  `harness = false` bench (`benches/revenue_sharing_benchmark.rs`) measures
  `record_revenue` at 2/5/20 participants and the insufficient-balance abort
  path, with deterministic event budgets and opt-in wall-clock gating. The event
  budgets are also asserted by `event_budget_scales_with_participants` in
  `src/test.rs`, so plain `cargo test` catches an event-volume change.

### Changed

- **`donation`, `withdrawal` and `search` storage schema bumped 1 → 2**
  (`[package.metadata.stellar-aid] storage-schema`).
- **`withdrawal::get_withdrawals_by_campaign` and the two `donation` history
  getters now return `(Vec<..>, PageInfo)` and take `start`/`limit`**, replacing
  the previous whole-list reads (#986).
- **`search`: `DataKey::AllArtists` is removed** and replaced by the
  `Index`/`IndexCount`/`ArtistIndex` layout. Existing listings are not migrated
  into the new index; `index_artist` must be called again to make an artist
  discoverable (#987).

### Fixed

- **`escrow::DataKey::AtomicCommit` can no longer expire mid-flow (#988).** The
  marker was previously written with no `extend_ttl` and no removal path, so a
  commit left `InProgress` could be archived before it was finalised.

## [0.1.0] — 2026-08-27

Initial documented baseline for every workspace contract (`shared`, `platform_config`, `escrow`, `commission_agreement`, `dispute_arbiter`, `messaging`, `subscription`, `competitions`, `verification`, `revenue_sharing`, `recruitment`, `creator_fund`, `campaign`, `donation`, `withdrawal`).

### Added

- Semantic version metadata in each `contracts/*/Cargo.toml` (`[package.metadata.stellar-aid]`).
- On-chain `get_version`, `get_version_metadata`, and `is_version_compatible` on all contracts (#682).
- `shared::version` constraint helpers and `CURRENT_STORAGE_SCHEMA`.
- Maintenance window, pause, backup, upgrade, and communication procedures (#681).

[Unreleased]: https://github.com/EDOHWARES/stellarAid-contract/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EDOHWARES/stellarAid-contract/releases/tag/v0.1.0
