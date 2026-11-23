# Donation contract decision

The pre-sweep `contracts/donation` crate shipped four near-identical files:
`src/lib.rs`, `src/doc.rs`, `src/clippy.rs`, and an unused `src/fee_config.rs`.
None was a workspace member, none was compiled, and the three `lib.rs` copies
were byte-for-byte copies of the same security-sensitive logic.

Choice: **delete the crate; do not register or productize it.**

- There is exactly zero copies of the duplicated donation implementation on
  disk. Donation value movement is now owned by the deployed flows in
  `contracts/campaign_factory` and `contracts/escrow`; adding a third donation
  entry point would reintroduce the duplication this issue is about.
- `fee_config.rs` is deleted rather than wired. Fee configuration is already
  part of the escrow and campaign factory initialize/configure paths, so a
  standalone module would be an unused second source of truth.
- The donation error-code band (`400-499`) and donation event type definitions
  remain in `contracts/shared` for indexer compatibility. Retaining codes and
  event schemas is not retaining a duplicate contract implementation.

This is a delete decision under issue #1082. The full path disposition is in
[CONTRACTS_TOP_LEVEL_DISPOSITION.md](./CONTRACTS_TOP_LEVEL_DISPOSITION.md).
