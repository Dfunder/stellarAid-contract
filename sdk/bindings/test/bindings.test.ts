// Smoke tests for the contract-interaction bindings (issue #866).
//
// The transaction-building client talks to Horizon/RPC over the network, so
// it is not exercisable here without a live network. What *is* testable, and
// what actually breaks in practice, is the argument encoding: `toScVal` is the
// seam between a JS application and every Soroban contract, and the event
// decoder the seam back.

import { scValToNative } from "@stellar/stellar-sdk";
import { describe, expect, it } from "vitest";

import { decodeRawEvents, toScVal } from "../src/contracts";

describe("toScVal", () => {
  it("encodes a plain i128 amount as an i128 ScVal that round-trips", () => {
    const encoded = toScVal({ type: "i128", value: 250000000n });
    expect(scValToNative(encoded)).toBe(250000000n);
  });

  it("leaves values without an explicit type to nativeToScVal", () => {
    // A plain number maps to an i64 by default; EscrowClient passes
    // {"type":"i128"} to be explicit about amount widths.
    const encoded = toScVal(7);
    expect(encoded).toBeDefined();
  });
});

describe("decodeRawEvents", () => {
  it("returns an empty list for garbage input", () => {
    expect(decodeRawEvents(null)).toEqual([]);
    expect(decodeRawEvents("not-an-array")).toEqual([]);
  });

  it("extracts decoded events and drops undecodable entries", () => {
    const raw = [
      { decoded: { topic: ["deposit"], data: { amount: 42 } } },
      { undecoded: true },
      null,
      42,
    ];
    expect(decodeRawEvents(raw)).toEqual([{ topic: ["deposit"], data: { amount: 42 } }]);
  });
});