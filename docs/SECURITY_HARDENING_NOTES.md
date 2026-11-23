# Security hardening notes: escrow expiry, admin setters, webhooks

Companion to the `partial_release` halt guard (#1126). Items below describe the required
behaviour; the `worker/` crate is not present in this checkout.

## Escrow expiry and auto-release (#1125)

- Store `expires_ledger` in `EscrowRecord` when the escrow is created.
- `expire_escrow`: allow the admin, or anyone only once `ledger().sequence() > expires_ledger`.
  Reject when `expires_ledger == 0`. Refund the locked amount; never strand it in `Expired`.
- `auto_release_on_deadline`: read the deadline from the stored record. Do not accept it as
  a caller argument.

## Admin-gated health/rollout setters (#1127)

Every setter that takes `admin: Address` must do both:

1. `admin.require_auth()`
2. compare `admin` with the stored admin and return `Unauthorized` on mismatch.

Put the comparison in one shared helper (for example in `contracts/shared/src/admin.rs`)
and call it from `trigger_rollback` and each contract's setters, so the check cannot be
forgotten per contract. Add one negative test per contract: a non-admin signer is rejected.

## worker/src/webhooks.rs (#1128)

- Sign the payload with HMAC-SHA256 and send the signature header; never send the shared
  secret itself.
- Verify with a constant-time comparison (`Mac::verify_slice`), not `==`.
- Insert into `delivered` only after a successful delivery, so transient failures retry.
- Bound the `delivered` set (LRU or capped size).
- Build the client with `Client::builder().timeout(Duration::from_secs(10))`.
