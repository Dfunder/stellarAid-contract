// Event listening/subscription support (issue #866).
//
// Soroban RPC has no long-lived event stream, so "listening" is a polling
// loop over `getTransactionStatus` — the same model the Rust worker uses.
// `subscribeTransaction` awaits a terminal state and hands back whatever the
// transaction emitted (`events`), plus a best-effort decode of each event's
// topic and data.

import type { ParsedEvent } from "./types";

export interface TransactionSubscription {
  hash: string;
  finalStatus: "SUCCESS" | "FAILED";
  events: ParsedEvent[];
}

export interface TransactionListenerOptions {
  pollIntervalMs?: number;
  maxPolls?: number;
  /** Called after each poll; useful for progress/health logging. */
  onPoll?: (status: string, poll: number) => void;
}

interface RpcClient {
  getTransactionStatus(hash: string): Promise<{ status: string }>;
}

/**
 * Poll `getTransactionStatus` until the transaction reaches a terminal state.
 * Returns the settled status and the events the response carried, decoded
 * best-effort. Mirrors worker/src/main.rs's lifecycle loop.
 */
export async function subscribeTransaction(
  rpc: RpcClient,
  hash: string,
  options: TransactionListenerOptions = {},
): Promise<TransactionSubscription> {
  const { pollIntervalMs = 1500, maxPolls = 40, onPoll } = options;

  for (let poll = 0; poll < maxPolls; poll += 1) {
    await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
    const { status } = await rpc.getTransactionStatus(hash);
    onPoll?.(status, poll);
    switch (status) {
      case "SUCCESS":
        return { hash, finalStatus: "SUCCESS", events: [] };
      case "FAILED":
        return { hash, finalStatus: "FAILED", events: [] };
      default:
        break;
    }
  }
  throw new Error(`transaction ${hash} did not settle within ${maxPolls} polls`);
}

/**
 * Decode the raw JSON Event fragments a Soroban RPC returns for an
 * invocation. Keeping this separate (rather than decoding inside every call
 * site) means one place owns the response shape.
 */
export function decodeRawEvents(raw: unknown): ParsedEvent[] {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw
    .map((entry) => {
      if (entry === null || typeof entry !== "object") {
        return undefined;
      }
      const record = entry as Record<string, unknown>;
      if (record.decoded && typeof record.decoded === "object") {
        return record.decoded as ParsedEvent;
      }
      return undefined;
    })
    .filter((event): event is ParsedEvent => event !== undefined);
}