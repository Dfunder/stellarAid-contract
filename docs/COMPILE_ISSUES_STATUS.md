# Compile issue status (current checkout)

This document records the status of assigned compile issues relative to the
current repository checkout.

## #912 platform_config unclosed delimiter

- Referenced path: `contracts/platform_config/src/lib.rs`
- Current state: `contracts/platform_config/` is not present on disk in this
  checkout, so the reported unclosed-delimiter parse error is not reproducible
  against current sources.
- Action: documented current state for traceability; when/if the crate is
  present, ensure `cargo check -p platform_config` parses/builds cleanly.

## #913 campaign finalize_withdrawal unclosed delimiter

- Referenced path: contracts/campaign/src/lib.rs
- Current state: contracts/campaign/ is not present on disk in this checkout,
  so the reported unclosed-delimiter parse error in inalize_withdrawal is not
  reproducible against current sources.
- Action: documented current state for traceability; when/if the crate is
  present, ensure cargo check -p campaign parses/builds cleanly and preserves
  intended inalize_withdrawal behavior.
