# Top-level `contracts/*.rs` disposition

After the post-sweep rebuild and the changes for #1079, there are **no**
top-level `.rs` files under `contracts/`. Every former top-level source file is
accounted for below. This table is the tracking document required by #1081.

| File | Disposition | Detail |
|------|-------------|--------|
| `contracts/semver_types.rs` | Deleted | Previously `include!`d by `campaign_factory`, `verification`, `multi_sig`, and `escrow`. Replaced by `shared::version` and `shared::impl_semver_queries!()` (#1079). |
| `contracts/escrow_multi_asset.rs` | Absent | Listed in issue #1081 but not present in the rebuilt tree; removed by the earlier sweep. |
| `contracts/creator_cooperative_dao.rs` | Absent | Same. |
| `contracts/creative_licensing.rs` | Absent | Same. |
| `contracts/dispute_resolution.rs` | Absent | Same. |
| `contracts/multi_chain_asset.rs` | Absent | Same. |
| `contracts/supported_assets.rs` | Absent | Same. |
| `contracts/talent_matching.rs` | Absent | Same. |
| `contracts/version_tracking.rs` | Absent | Same. The issue names this file twice; one unique path is tracked. |
| `contracts/creator_grants.rs` | Absent | Same. |
| `contracts/cross_chain_bridge.rs` | Absent | Same. |
| `contracts/platform_config_upgrade.rs` | Absent | Same. |
| `contracts/platform_fee_override.rs` | Absent | Same. |
| `contracts/escrow_tip_payment.rs` | Absent | Same. |
| `contracts/escrow_emergency_pause.rs` | Absent | Same. |
| `contracts/commission_signature.rs` | Absent | Same. |

No experimental crate is registered for these paths because no source remains
to quarantine. The workspace `members` list was reduced to the five crates
that exist on disk, so `cargo build --workspace` no longer fails when it reads
the manifest. See [VERSIONING.md](./VERSIONING.md) for the canonical semver
implementation and [ENV_REFERENCE.md](./ENV_REFERENCE.md) for configuration.
