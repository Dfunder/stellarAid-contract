# @stellaraid/wallet-adapters

Unified wallet-adapter interface plus contract-interaction bindings for the
StellarAid contracts.

## Wallet adapters (`src/wallet`)

One `WalletAdapter` interface implemented for Freighter, Albedo and Lobstr, so
application code never touches a wallet vendor's API directly:

```ts
import { getDefaultAdapter } from "../src/index";

const wallet = getDefaultAdapter();
const publicKey = await wallet.connect();
const signedXdr = await wallet.sign(unsignedEnvelopeXdr);
```

* `connect()` — prompt for the user's `G...` public key.
* `sign(xdr)` — sign a base64 envelope and return the signed XDR.
* `disconnect()`, `isAvailable()` — session and detection helpers.

## Contract-interaction bindings (`src/contracts`, issue #866)

The adapters sign; `ContractClient` turns a method name and arguments into a
Soroban invocation and reads the result:

```ts
import { ContractClient, EscrowClient, TESTNET, type ContractClientConfig } from "../src/index";

const client = await EscrowClient.for({
  contractId: "C...",
  network: TESTNET,
  source: "G...",
  wallet: getDefaultAdapter(),
});

// simulate without paying, to show cost + events:
const sim = await client.simulateCreateEscrow("commission-1", from, to, 1_000_000_000);
console.log(sim.minResourceFee, sim.events);

// or build + sign + submit + poll to a terminal state:
const receipt = await client.simulateOpenDispute("commission-1", from);
```

Per-contract typed clients ([`contracts.ts`](src/contracts/contracts.ts)) pin the
on-chain signatures from `contracts/` for escrow, commission agreements,
disputes and campaigns. `simulate`/`send` mirror the Rust SDK's flow: fetch the
source account from Horizon, build a single invoke transaction, simulate for a
realistic fee, sign through the wallet, submit and poll
`getTransactionStatus`.

### Argument encoding

`toScVal` converts JS values to Soroban `ScVal`s. StellarAid amounts are
`i128`, so they are passed explicitly:

```ts
client.simulate("deposit", [subscriber, { type: "i128", value: 10_0000000n }]);
```

### Events (`events.ts`)

`subscribeTransaction` is the "listening for events" surface — a polling loop
over `getTransactionStatus` matching the worker's lifecycle handling.
`decodeRawEvents` converts the RPC's raw event fragments into `ParsedEvent`s.

## Checks

```bash
npm run check   # typecheck + lint + test + build
```