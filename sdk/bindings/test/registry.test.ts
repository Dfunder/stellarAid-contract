// Smoke tests for the wallet adapter registry.
//
// Closes #885 — `npm test` needs something to run, and "it compiles" is not
// evidence that the registry behaves.
//
// WHAT IS AND IS NOT COVERED
//   The adapters themselves talk to three browser extensions, so `connect()`
//   and `sign()` are not exercisable here without a real wallet. What *is*
//   testable, and what actually breaks in practice, is the registry: which
//   adapter is picked, how a name lookup behaves, and — the part that matters
//   most — that detection never throws when a wallet is absent.
//
// DETECTION, AS THE SOURCE ACTUALLY IMPLEMENTS IT
//   Freighter  window.freighter        !== undefined
//   Albedo     any `window` at all      (it is a popup, not an extension)
//   Lobstr     window.lobstrExtension  !== undefined
//
//   That asymmetry is load-bearing for the tests below: under vitest's default
//   `node` environment there is no `window`, so every adapter must report
//   unavailable without throwing — but as soon as a `window` exists, Albedo
//   wins by default even with no extension present. Asserting anything else
//   here would be asserting a behaviour the code does not have.
import { afterEach, describe, expect, it } from "vitest";

import {
  AlbedoAdapter,
  FreighterAdapter,
  LobstrAdapter,
  WalletAdapterRegistry,
  getAdapterByName,
  getDefaultAdapter,
} from "../src/index";

interface GlobalWithWindow {
  window?: Record<string, unknown>;
}

/** Present a set of wallet globals, as a browser host with those extensions would. */
function withWindow(globals: Record<string, unknown>): void {
  (globalThis as unknown as GlobalWithWindow).window = globals;
}

function clearWindow(): void {
  delete (globalThis as unknown as GlobalWithWindow).window;
}

afterEach(() => {
  clearWindow();
});

describe("WalletAdapterRegistry", () => {
  it("contains exactly the three supported adapters, in priority order", () => {
    // Order is the priority order: getDefaultAdapter() returns the first
    // match, so a reordering here silently changes which wallet a user gets.
    expect(WalletAdapterRegistry.map((a) => a.name)).toEqual([
      "Freighter",
      "Albedo",
      "Lobstr",
    ]);
  });

  it("exposes every registry member as a WalletAdapter", () => {
    for (const adapter of WalletAdapterRegistry) {
      expect(typeof adapter.connect).toBe("function");
      expect(typeof adapter.sign).toBe("function");
      expect(typeof adapter.disconnect).toBe("function");
      expect(typeof adapter.isAvailable).toBe("function");
    }
  });
});

describe("detection", () => {
  it("reports every adapter unavailable when no window is defined", () => {
    // No window at all — the default vitest environment. A regression to a
    // bare `window.x` here throws a ReferenceError, which would take down
    // server-side rendering and any Node consumer of this package.
    for (const adapter of WalletAdapterRegistry) {
      expect(() => adapter.isAvailable()).not.toThrow();
      expect(adapter.isAvailable()).toBe(false);
    }
  });

  it("reports every adapter unavailable for an empty window", () => {
    withWindow({});
    expect(new FreighterAdapter().isAvailable()).toBe(false);
    expect(new LobstrAdapter().isAvailable()).toBe(false);
  });

  it("detects Freighter from window.freighter", () => {
    withWindow({ freighter: {} });
    expect(new FreighterAdapter().isAvailable()).toBe(true);
    expect(new LobstrAdapter().isAvailable()).toBe(false);
  });

  it("detects Lobstr from window.lobstrExtension, not window.lobstr", () => {
    // The extension's global is `lobstrExtension`. A wallet named "Lobstr"
    // that injected `window.lobstr` would otherwise be silently missed.
    withWindow({ lobstrExtension: {} });
    expect(new LobstrAdapter().isAvailable()).toBe(true);

    clearWindow();
    withWindow({ lobstr: {} });
    expect(new LobstrAdapter().isAvailable()).toBe(false);
  });

  it("treats Albedo as available for any window, because it is a popup", () => {
    withWindow({});
    expect(new AlbedoAdapter().isAvailable()).toBe(true);
  });
});

describe("getDefaultAdapter", () => {
  it("returns null when there is no window at all", () => {
    expect(getDefaultAdapter()).toBeNull();
  });

  it("prefers Freighter when both Freighter and Albedo are present", () => {
    // Both are available whenever a window exists, so this asserts the
    // registry order rather than any capability difference.
    withWindow({ freighter: {} });
    expect(getDefaultAdapter()?.name).toBe("Freighter");
  });

  it("falls through to Albedo when Freighter is absent", () => {
    withWindow({});
    expect(getDefaultAdapter()).toBeInstanceOf(AlbedoAdapter);
  });

  it("skips an unavailable adapter ahead of an available one", () => {
    withWindow({ freighter: undefined, albedo: {} });
    expect(getDefaultAdapter()).toBeInstanceOf(AlbedoAdapter);
  });
});

describe("getAdapterByName", () => {
  it("is case-insensitive", () => {
    for (const spelling of ["freighter", "FREIGHTER", "FrEiGhTeR"]) {
      expect(getAdapterByName(spelling)).toBeInstanceOf(FreighterAdapter);
    }
  });

  it("resolves the other adapters by name", () => {
    expect(getAdapterByName("Albedo")).toBeInstanceOf(AlbedoAdapter);
    expect(getAdapterByName("Lobstr")).toBeInstanceOf(LobstrAdapter);
  });

  it("returns null for an unknown or empty name rather than throwing", () => {
    expect(getAdapterByName("Phantom")).toBeNull();
    expect(getAdapterByName("")).toBeNull();
  });

  it("resolves by name independently of availability", () => {
    // A name lookup is explicit caller intent, so it must not be filtered by
    // what happens to be installed.
    expect(getAdapterByName("Lobstr")).not.toBeNull();
  });
});
