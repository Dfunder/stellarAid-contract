# dispute_arbiter: hardening requirements

`contracts/dispute_arbiter` is not present in this checkout. This note records the fixes
required when the crate lands, so reviewers have one place to check them against.

## partial_resolve (#1130)

1. Split the escrow amount first, then pay out. Never call `refund_client` for 100% and
   compute shares from the balance afterwards.
2. Compute `client_share` and `artist_share` from the commission's own escrow record
   (`amount - released_amount`), not from the escrow contract's token balance. The token
   balance covers every commission and must never be used as a split base.
3. Pay the real `client` and `artist` addresses read from the escrow record. A
   `commission_id` (`Bytes`) is not an `Address` and must never be converted into one.
4. Move funds through an authorised escrow entry point (admin/arbiter role checked by
   escrow), not by letting the arbiter transfer from the escrow contract directly.
5. `client_share + artist_share` must equal the remaining amount exactly.

## Authentication (#1131)

- `open_dispute`: read the escrow record by `commission_id`, require the caller to be the
  record's `client` or `artist`, and call `require_auth()` on that address.
- `auto_resolve`: require the arbiter/admin, or only allow it once the dispute deadline has
  passed. Document the outcome rule in code and here, and add a test for each outcome.

## Tests to add with the crate

- Non-party `open_dispute` is rejected; client and artist are accepted.
- `partial_resolve` pays the exact shares and leaves other commissions untouched.
- `auto_resolve` is rejected for an unauthorised caller or before the deadline.
