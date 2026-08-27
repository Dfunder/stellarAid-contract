# Changelog

All notable changes to Lumora / StellarAid contracts are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
How versions are stored, queried, and constrained is defined in [docs/VERSIONING.md](docs/VERSIONING.md).

## [Unreleased]

## [0.1.0] — 2026-08-27

Initial documented baseline for every workspace contract (`shared`, `platform_config`, `escrow`, `commission_agreement`, `dispute_arbiter`, `messaging`, `subscription`, `competitions`, `verification`, `revenue_sharing`, `recruitment`, `creator_fund`, `campaign`, `donation`, `withdrawal`).

### Added

- Semantic version metadata in each `contracts/*/Cargo.toml` (`[package.metadata.stellar-aid]`).
- On-chain `get_version`, `get_version_metadata`, and `is_version_compatible` on all contracts (#682).
- `shared::version` constraint helpers and `CURRENT_STORAGE_SCHEMA`.
- Maintenance window, pause, backup, upgrade, and communication procedures (#681).

[Unreleased]: https://github.com/EDOHWARES/stellarAid-contract/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EDOHWARES/stellarAid-contract/releases/tag/v0.1.0
