# Contract inventory and conventions

## Canonical campaign factory

`contracts/campaign_factory` is the single canonical factory. The older
`contracts/factory` crate was deleted in the post-sweep rebuild; the inline
`register_contract(None, CampaignContract)` model is not used.

The canonical path stores the campaign WASM hash in instance storage through
`initialize`/`update_wasm_hash`, derives a deterministic salt with
`env.crypto().sha256(&params)`, and deploys with
`env.deployer().with_address(creator, salt).deploy(hash)`. Tests only use host
contracts and in-memory hashes; they do not depend on pre-built `target/`
artifacts, so `cargo test -p campaign-factory` passes on a fresh clone.

## Typed errors

All deployed contracts use `#[contracterror]` enums and return
`Result<T, ContractError>` from fallible entry points. The numeric bands in
`contracts/shared/src/errors.rs` are part of the on-chain ABI:

- `1-99`: domain-local errors (`campaign_factory` pins `1-4`)
- `100-199`: escrow
- `200-299`: agreements
- `300-399`: campaigns
- `400-499`: donations
- `500-599`: configuration
- `600-699`: disputes

The pre-sweep `contracts/campaign` crate had `panic!`/`expect(` paths and has
been deleted. Its successor does not contain non-test `panic!` or `expect(`
calls; `contracts/campaign_factory/src/test.rs::error_codes_are_stable` pins
the campaign-factory codes and the shared campaign band so a future edit
cannot silently renumber them. If a new campaign entry point is introduced,
it must return `Result<_, E>` with a `#[contracterror]` `E` and add a code
stability test.

## Removed paths

`contracts/donation` is deleted with the rationale in
[DONATION_CONTRACT_DECISION.md](./DONATION_CONTRACT_DECISION.md). The seven
dead `worker/src/*.ts` files are deleted; the worker runtime decision is in
[../worker/README.md](../worker/README.md).
