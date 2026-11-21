// ContractClient: build, simulate, sign and submit Soroban invocations
// (issue #866).
//
// The client is the transaction-building half of the package. It mirrors the
// Rust SDK's flow exactly — fetch the source account from Horizon, build a
// single invoke-host-function transaction, simulate against the RPC for a
// realistic fee, then (for `send`) hand the envelope to the wallet adapter,
// submit it and poll to a terminal state. All network I/O is plain fetch, so
// the client works in node and in the browser.

import { Account, Operation, TransactionBuilder, nativeToScVal } from "@stellar/stellar-sdk";

import {
  type ContractClientConfig,
  type InvocationResult,
  type ParsedEvent,
} from "./types";

export interface Simulation {
  /** Decoded return value of the call, when the simulation produced one. */
  result?: unknown;
  /** Fee the simulation recommends, in stroops. */
  minResourceFee: number;
  /** Human-readable error when the invocation failed to simulate. */
  error?: string;
  /** Events emitted during simulation, decoded best-effort. */
  events: ParsedEvent[];
  /** Raw cost JSON as returned by the RPC, for inspection. */
  rawCost?: unknown;
}

export interface SendReceipt {
  hash: string;
  status: string;
}

const ERROR_MSG = "RPC error";

interface RpcResponse {
  jsonrpc?: string;
  id?: number;
  result?: unknown;
  error?: { code?: number; message?: string };
}

/** Minimal Soroban JSON-RPC client; mirrors sdk/src/soroban/rpc_client.rs. */
export class SorobanRpcClient {
  constructor(private readonly rpcUrl: string) {}

  private async call(method: string, params: Record<string, unknown>): Promise<unknown> {
    const response = await fetch(this.rpcUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: Date.now(),
        method,
        params,
      }),
    });
    if (!response.ok) {
      throw new Error(`${method}: ${ERROR_MSG} ${response.status}`);
    }
    const body = (await response.json()) as RpcResponse;
    if (body.error) {
      throw new Error(`${method}: ${body.error.message ?? ERROR_MSG} (${body.error.code ?? "unknown"})`);
    }
    return body.result;
  }

  simulateTransaction(envelopeXdr: string): Promise<unknown> {
    return this.call("simulateTransaction", { transaction: envelopeXdr });
  }

  sendTransaction(envelopeXdr: string): Promise<SendReceipt> {
    return this.call("sendTransaction", { transaction: envelopeXdr }) as Promise<SendReceipt>;
  }

  getTransactionStatus(hash: string): Promise<{ status: string }> {
    return this.call("getTransactionStatus", { hash }) as Promise<{ status: string }>;
  }
}

/** Horizon account lookup, matching the Rust SDK's HorizonClient. */
async function fetchAccountSequence(horizonUrl: string, address: string): Promise<number> {
  const response = await fetch(`${horizonUrl}/accounts/${address}`);
  if (!response.ok) {
    throw new Error(`horizon: fetching ${address} failed with ${response.status}`);
  }
  const body = (await response.json()) as { sequence?: string };
  const sequence = Number(body.sequence);
  if (!Number.isSafeInteger(sequence)) {
    throw new Error(`horizon: unexpected sequence for ${address}`);
  }
  return sequence;
}

/**
 * Convert a JS value into a Soroban `ScVal`. Plain numbers/strings/booleans
 * map to their obvious `ScVal`s; `{ type: "i128", value: bigint }` selects a
 * wide integer (StellarAid amounts are i128).
 */
export function toScVal(value: unknown): ReturnType<typeof nativeToScVal> {
  if (value !== null && typeof value === "object" && "type" in value && value.type === "i128") {
    return nativeToScVal((value as unknown as { value: bigint | string }).value, {
      type: "i128",
    });
  }
  return nativeToScVal(value);
}

/**
 * Connect a wallet to a contract.
 *
 * `ContractClient.for` fetches nothing; the network costs happen on the first
 * `simulate`/`send`. Signing is delegated to the supplied wallet adapter.
 */
export class ContractClient {
  private readonly rpc: SorobanRpcClient;

  private constructor(private readonly config: ContractClientConfig) {
    this.rpc = new SorobanRpcClient(config.network.rpcUrl);
  }

  static async for(config: ContractClientConfig): Promise<ContractClient> {
    // Kept async so a future credential/key check can run here without a
    // breaking signature change.
    return new ContractClient(config);
  }

  get contractId(): string {
    return this.config.contractId;
  }

  getSource(): string {
    return this.config.source;
  }

  /** Simulate a call, returning the decoded return value, events and cost. */
  async simulate(method: string, args: unknown[]): Promise<Simulation> {
    // Fetch the source once per call so a simulated sequence does not fight a
    // concurrent submission that already bumped it.
    const sequence = await fetchAccountSequence(this.config.network.horizonUrl, this.config.source);
    const envelopeXdr = await this.buildInvocation(method, args, sequence);
    const raw = (await this.rpc.simulateTransaction(envelopeXdr)) as {
      error?: string;
      results?: Array<{ SvalueDecoration?: unknown; _ValueDecoration?: unknown }>;
      cost?: unknown;
      minResourceFee?: number;
      events?: Array<{ decoded?: ParsedEvent }>;
    };
    if (raw.error) {
      return { minResourceFee: 0, error: raw.error, events: [] };
    }
    const result = this.firstDecodedResult(raw.results);
    return {
      result,
      minResourceFee: raw.minResourceFee ?? 0,
      events: this.decodeEvents(raw.events),
      rawCost: raw.cost,
    };
  }

  /** Build + simulate + sign (through the wallet adapter) + submit + poll. */
  async send(method: string, args: unknown[]): Promise<InvocationResult<unknown>> {
    const sequence = await fetchAccountSequence(this.config.network.horizonUrl, this.config.source);
    const unsigned = await this.buildInvocation(method, args, sequence);
    const signed = await this.config.wallet.sign(unsigned);
    const { hash } = await this.rpc.sendTransaction(signed);
    // Poll to a terminal state before returning, like the worker's request
    // loop — a transaction has not "happened" until it settles.
    const status = await this.pollUntilTerminal(hash);
    return { value: { hash, status }, events: [] };
  }

  /** Poll getTransactionStatus to a terminal state. */
  private async pollUntilTerminal(hash: string, maxPolls = 40, intervalMs = 1500): Promise<string> {
    for (let poll = 0; poll < maxPolls; poll += 1) {
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
      const { status } = await this.rpc.getTransactionStatus(hash);
      if (status === "SUCCESS" || status === "FAILED") {
        return status;
      }
    }
    throw new Error(`transaction ${hash} did not settle in ${maxPolls} polls`);
  }

  /** Build a single-operation invoke transaction and return its envelope XDR. */
  private async buildInvocation(method: string, args: unknown[], sequence: number): Promise<string> {
    const account = new Account(this.config.source, String(sequence + 1));
    const operation = Operation.invokeContractFunction({
      contract: this.config.contractId,
      function: method,
      args: args.map((arg) => toScVal(arg)),
    });
    const transaction = new TransactionBuilder(account, {
      fee: "100000",
      networkPassphrase: this.config.network.networkPassphrase,
    })
      .addOperation(operation)
      .setTimeout(0)
      .build();

    return transaction.toXDR();
  }

  private firstDecodedResult(
    results?: Array<{ SvalueDecoration?: unknown; _ValueDecoration?: unknown }>,
  ): unknown {
    if (!results || results.length === 0) {
      return undefined;
    }
    const first = results[0] as unknown;
    if (first === null || typeof first !== "object") {
      return first;
    }
    const record = first as Record<string, unknown>;
    return record.SvalueDecoration ?? record._ValueDecoration;
  }

  private decodeEvents(raw?: Array<{ decoded?: ParsedEvent }>): ParsedEvent[] {
    if (!raw) {
      return [];
    }
    return raw
      .map((event) => event.decoded)
      .filter((event): event is ParsedEvent => event !== undefined);
  }
}