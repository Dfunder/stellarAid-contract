# @stellaraid/wallet-adapters

Unified wallet-adapter interface plus contract-interaction bindings for the
StellarAid contracts.

## Wallet adapters (`src/wallet`)

One `WalletAdapter` interface implemented for Freighter, Albedo and Lobstr, so
application code never touches a wallet vendor's API directly:

```ts
import { getDefaultAdapter } from "../src/index";

const wallet = getDefaultAdapter();
if (!wallet) {
  // Outside a browser (Node, SSR, tests) no adapter is available.
  throw new Error("No Stellar wallet available in this environment");
}
const publicKey = await wallet.connect();
const signedXdr = await wallet.sign(unsignedEnvelopeXdr);
```

* `connect()`: prompts for the user's `G...` public key.
* `sign(xdr)`: signs a base64 envelope and returns the signed XDR.
* `disconnect()`, `isAvailable()`: session and detection helpers.

`getDefaultAdapter()` returns `WalletAdapter | null`. It picks the first
available adapter in registry order (Freighter, Albedo, Lobstr) and returns
`null` when none is available, so always check the result before using it.

Albedo is popup-based and reports itself available whenever `window` exists.
In any browser without Freighter installed, `getDefaultAdapter()` therefore
returns Albedo, never Lobstr. Don't treat a non-null result as proof that a
wallet extension is installed. To use a specific wallet, select it by name and
check `isAvailable()` yourself:

```ts
import { getAdapterByName } from "../src/index";

const lobstr = getAdapterByName("Lobstr");
if (!lobstr?.isAvailable()) {
  throw new Error("Lobstr is not available");
}
```

## Contract-interaction bindings (`src/contracts`, issue #866)

The adapters sign; `ContractClient` turns a method name and arguments into a
Soroban invocation and reads the result:

```ts
import {
  ContractClient,
  EscrowClient,
  TESTNET,
  getDefaultAdapter,
  type ContractClientConfig,
} from "../src/index";

const wallet = getDefaultAdapter();
if (!wallet) throw new Error("No Stellar wallet available");

const config: ContractClientConfig = {
  contractId: "C...",
  network: TESTNET,
  source: "G...",
  wallet, // must be a WalletAdapter, never null
};

// Simulate without paying, to see cost and events. Nothing is submitted.
const escrow = await EscrowClient.for(config);
const sim = await escrow.simulateCreateEscrow("commission-1", from, to, 1_000_000_000);
console.log(sim.minResourceFee, sim.events);

// Dry-run a dispute. simulate* methods NEVER submit a transaction.
const disputeSim = await escrow.simulateOpenDispute("commission-1", from);
if (disputeSim.error) throw new Error(disputeSim.error);

// Submit for real with ContractClient.send: build, sign through the wallet,
// submit, then poll until the transaction settles.
const client = await ContractClient.for(config);
const receipt = await client.send("open_dispute", ["commission-1", from]);
console.log(receipt.value); // { hash, status: "SUCCESS" | "FAILED" }
```

Per-contract typed clients ([`contracts.ts`](src/contracts/contracts.ts)) pin the
on-chain signatures from `contracts/` for escrow, commission agreements,
disputes and campaigns. They expose only `simulate*` methods and read-only
getters. To submit a transaction, call `ContractClient.send(method, args)`
with the same method name and arguments.

* `simulate` fetches the source account from Horizon, builds a single invoke
  transaction and simulates it to report the result, events and fee. It never
  signs or submits.
* `send` builds the invoke transaction, signs it through the wallet adapter,
  submits it and polls `getTransactionStatus` until it reaches a terminal state.

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