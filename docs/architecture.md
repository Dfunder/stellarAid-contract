# StellarAid Contract Architecture

This document maps the workspace as defined in the root `Cargo.toml` and the
current checkout state. It is the high-level system map for contributors.

## Workspace members (source of truth: `Cargo.toml`)

| Path | Kind | Status | Notes |
| --- | --- | --- | --- |
| contracts/shared | contract/library crate (workspace member) | present on disk | Shared types/utilities used by contract crates. |
| contracts/platform_config | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/escrow | contract (workspace member) | present on disk | Escrow state machine and fund flows (lock/release/refund/dispute/cancel/partial release). |
| contracts/commission_agreement | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/dispute_arbiter | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/analytics | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/reputation | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/messaging | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/subscription | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/competitions | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/verification | contract (workspace member) | present on disk | Verification-related contract crate. |
| contracts/search | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/revenue_sharing | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/recruitment | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/creator_fund | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/rate_limiter | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/multi_sig | contract (workspace member) | present on disk | Multi-signature contract crate. |
| contracts/audit | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/nft | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/dao | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/ecosystem_funding | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/licensing | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/mentorship | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/campaign | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/campaign_factory | contract (workspace member) | present on disk | Campaign factory contract crate. |
| contracts/campaign_instance | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| contracts/factory | contract (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| sdk | sdk (workspace member) | present on disk | TypeScript/SDK utilities and bindings-related code. |
| tests/framework | test crate (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |
| tests/upgrade_testing | test crate (workspace member) | missing on disk (listed in Cargo.toml) | Listed in workspace; directory not present in this checkout. |

> **Note:** The table above includes every workspace member declared in the root
> `Cargo.toml` at the time of this document. Several listed members are not
> present on disk in this checkout; keep this document and `Cargo.toml` aligned
> when members are added/removed or restored.

## Unregistered / not-workspace crates

- `contracts/donation` and `contracts/withdrawal`: referenced by the root
  `Cargo.toml` note as on-disk but not workspace members (unaudited/unregistered)
  in prior states; **not present on disk in this checkout**. They are not built
  by `cargo build --workspace` unless/until added, audited, and registered.
- `worker/`, `multi_wallet_session/`, and `tests/wallet_integration/`: **not
  present on disk in this checkout** and not workspace members (see
  `CONTRIBUTING.md` and `Cargo.toml` notes for coverage decisions).

## Present components and flows

### On-disk workspace components

- `contracts/shared`: shared contract utilities/types.
- `contracts/escrow`: central escrow contract managing commission funds and
  lifecycle transitions (creation, release/refund/expiry/dispute handling,
  cancellation/settlement and related invariants) with token interactions.
- `contracts/multi_sig`: multi-signature authorization/support contract.
- `contracts/verification`: verification-related contract.
- `contracts/campaign_factory`: campaign factory contract.
- `sdk`: off-chain SDK/bindings utilities.

### Primary flow (as implemented by present crates)

```text
Client / Caller
      |
      v
[ escrow ] <---- uses ---- [ shared ]
      |
      +-- token transfers (via Soroban token interface)
      +-- emits events / enforces state & balance invariants

[ campaign_factory ]  (campaign-related factory contract, present)
[ verification ]      (verification contract, present)
[ multi_sig ]         (multi-signature contract, present)
```

Cross-contract integrations described in historical issues/docs (for example
commission agreement, dispute arbiter, campaign/donation/withdrawal pipelines,
factory WASM imports) depend on crates not present in this checkout and are not
fully represented by on-disk code here. Refer to `Cargo.toml`, git history, and
issue/PR discussions for the intended end-to-end architecture when those
components are present.

## Consistency

- Workspace member list source of truth: root `Cargo.toml` `[workspace].members`.
- This document should be updated whenever workspace membership or on-disk
  contract layout changes.
- `docs/CONTRACTS.md` is not present in this checkout; if added/restored, keep it
  consistent with `Cargo.toml` and this architecture map.
