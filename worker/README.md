# Worker runtime

The **Rust worker is canonical** for this repository. The seven pre-sweep
TypeScript files are deleted:

- `src/health.ts`
- `src/webhook_hmac.ts`
- `src/webhook_sig_verify.ts`
- `src/webhook_retry.ts`
- `src/batch_verify.ts`
- `src/bulk_donation_status.ts`
- `src/rate_limit_status.ts`

They were never compiled or tested, were mutually inconsistent (the HMAC
signer and verifier used different payloads), and pointed at endpoints that do
not exist. There is no `worker/ts/` productization path: adding a second
runtime would recreate the exact ambiguity this decision removes.

Verify the deletion on a clean clone:

```sh
grep -R --include='*.ts' . worker/ || true
```

The command must print nothing. Integrators should use the Rust worker and the
deploy scripts in `scripts/deploy/` rather than importing any TypeScript from
this directory.
