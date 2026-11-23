import type { WalletAdapter } from "./types";
import { FreighterAdapter } from "./freighter";
import { AlbedoAdapter } from "./albedo";
import { LobstrAdapter } from "./lobstr";

/**
 * All supported wallet adapters.
 * Switching the active wallet requires no code change — pick any entry
 * that satisfies `adapter.isAvailable()`.
 */
export const WalletAdapterRegistry: readonly WalletAdapter[] = [
  new FreighterAdapter(),
  new AlbedoAdapter(),
  new LobstrAdapter(),
] as const;

/**
 * Return the first adapter that reports itself as available, or null if none
 * are detectable in the current environment.
 */
export function getDefaultAdapter(): WalletAdapter | null {
  return WalletAdapterRegistry.find((a) => a.isAvailable()) ?? null;
}

/**
 * Look up an adapter by its display name (case-insensitive).
 */
export function getAdapterByName(name: string): WalletAdapter | null {
  return (
    WalletAdapterRegistry.find(
      (a) => a.name.toLowerCase() === name.toLowerCase()
    ) ?? null
  );
}
