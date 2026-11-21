# StellarAid Contract Architecture

This document maps the real contract/workspace layout in this checkout. The
source of truth for Rust workspace membership is the root `Cargo.toml`.

## Workspace members

| Path | Kind | Status | Notes |
| --- | --- | --- | --- |
| contracts/shared | contract/library crate | present (workspace member) | Shared types/utilities; has `[package.metadata.stellar-aid]`. |
| contracts/escrow | contract crate | present (workspace member) | Escrow lifecycle and fund-flow contract; has `[package.metadata.stellar-aid]`. |
| contracts/verification | contract crate | present (workspace member) | Verification contract; has `[package.metadata.stellar-aid]`. |
| contracts/multi_sig | contract crate | present (workspace member) | Multi-signature contract; has `[package.metadata.stellar-aid]`. |
| contracts/campaign_factory | contract crate | present (workspace member) | Campaign factory contract; has `[package.metadata.stellar-aid]`. |

All contract directories present on disk in this checkout are registered as
workspace members in `Cargo.toml`, and every registered crate above includes a
`[package.metadata.stellar-aid]` block.

## Not present / not registered

- `contracts/campaign`: not present on disk in this checkout and not a workspace
  member.
- `contracts/donation`, `contracts/withdrawal`: not present on disk in this
  checkout and not registered workspace members. If/when added, they must be
  audited, given `[package.metadata.stellar-aid]`, and registered (or explicitly
  excluded with rationale) before being built.
- Other contract/crate paths referenced by historical issues or prior workspace
  states (e.g. `platform_config`, `commission_agreement`, `dispute_arbiter`,
  `dao`, `factory`, `campaign_instance`, `tests/framework`,
  `tests/upgrade_testing`, Rust `sdk`) are not present as registered workspace
  crates in this checkout.

## Present component flow (high level)

```text
Client / Caller
      |
      v
[ escrow ] <---- uses ---- [ shared ]
      |
      +-- token interactions and escrow state transitions/events

[ campaign_factory ]  campaign factory contract (present)
[ verification ]      verification contract (present)
[ multi_sig ]         multi-signature contract (present)
```

This checkout also contains non-workspace support directories (e.g. `sdk/`
TypeScript utilities, `scripts/`, `docs/`) that are not Rust workspace members.

## Consistency

- Workspace membership: root `Cargo.toml`
- Contract set documented here must match the registered crates present on disk.
- `docs/CONTRACTS.md` is not present in this checkout; if added, keep it
  consistent with `Cargo.toml` and this document.
