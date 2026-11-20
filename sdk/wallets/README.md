# Freighter adapter migration

`wallets/freighter.ts` is the single Freighter implementation. It targets
`@stellar/freighter-api` **^3.0.0**, which is the only Freighter major in the
dependency graph (`sdk/package.json` and `sdk/bindings/package.json`).

The former `bindings/src/wallet/freighter.ts` adapter used a `window.freighter`
shim and peer `^1.7.1`; it is now a re-export of this module. If you imported
from the bindings path, your imports keep working, but new code should import
from `sdk/wallets/freighter.ts`.

`signTransactionWithFreighter(xdr, networkPassphrase)` always forwards the
passphrase to the v3 API. `FreighterAdapter` accepts a default passphrase in
its constructor and still lets each `sign()` call override it.
