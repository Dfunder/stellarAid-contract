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

## [0.1.0] — 2026-08-27

Initial documented baseline for every workspace contract (`shared`, `platform_config`, `escrow`, `commission_agreement`, `dispute_arbiter`, `messaging`, `subscription`, `competitions`, `verification`, `revenue_sharing`, `recruitment`, `creator_fund`, `campaign`, `donation`, `withdrawal`).

### Added

- Semantic version metadata in each `contracts/*/Cargo.toml` (`[package.metadata.stellar-aid]`).
- On-chain `get_version`, `get_version_metadata`, and `is_version_compatible` on all contracts (#682).
- `shared::version` constraint helpers and `CURRENT_STORAGE_SCHEMA`.
- Maintenance window, pause, backup, upgrade, and communication procedures (#681).

[Unreleased]: https://github.com/EDOHWARES/stellarAid-contract/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EDOHWARES/stellarAid-contract/releases/tag/v0.1.0
