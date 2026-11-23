/**
 * Canonical Freighter adapter (closes #1087).
 *
 * This is the only Freighter implementation in the repository. The legacy
 * `sdk/bindings` path re-exports this module; do not add a `window.freighter`
 * shim there. Built against @stellar/freighter-api ^3 (one major in the graph).
 */

import {
  isBrowser,
  isConnected,
  requestAccess,
  signTransaction,
} from '@stellar/freighter-api';

export const FREIGHTER_DOWNLOAD_URL = 'https://www.freighter.app/';

export const NETWORK_PASSPHRASE = {
  MAINNET: 'Public Global Stellar Network ; September 2015',
  TESTNET: 'Test SDF Network ; September 2015',
} as const;

export type StellarNetwork =
  (typeof NETWORK_PASSPHRASE)[keyof typeof NETWORK_PASSPHRASE];

export class FreighterNotInstalledError extends Error {
  readonly downloadUrl = FREIGHTER_DOWNLOAD_URL;

  constructor() {
    super(
      `Freighter wallet extension is not installed. Download it at ${FREIGHTER_DOWNLOAD_URL}`
    );
    this.name = 'FreighterNotInstalledError';
  }
}

/** Wallet shape shared with SDK consumers. */
export interface WalletAdapter {
  readonly name: string;
  isAvailable(): boolean;
  connect(): Promise<string>;
  sign(xdr: string, networkPassphrase?: string): Promise<string>;
  disconnect(): void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : 'unknown Freighter error';
}

export function isFreighterInstalled(): boolean {
  return Boolean(isBrowser) && typeof window !== 'undefined' && 'freighter' in window;
}

/** Request the user's public key. */
export async function connectFreighter(): Promise<string> {
  if (!isFreighterInstalled()) throw new FreighterNotInstalledError();

  const connectionCheck = await isConnected();
  if (connectionCheck.error) {
    throw new Error(`Freighter connection check failed: ${messageOf(connectionCheck.error)}`);
  }

  const { address, error } = await requestAccess();
  if (error) throw new Error(`Freighter access denied: ${messageOf(error)}`);
  if (!address) throw new Error('Freighter did not return a public key.');
  return address;
}

/**
 * Sign an XDR envelope. `networkPassphrase` is passed through to the v3 API —
 * never dropped or defaulted silently inside the adapter.
 */
export async function signTransactionWithFreighter(
  xdr: string,
  networkPassphrase: string
): Promise<string> {
  if (!isFreighterInstalled()) throw new FreighterNotInstalledError();

  const { signedTxXdr, error } = await signTransaction(xdr, { networkPassphrase });
  if (error) throw new Error(`Freighter signing failed: ${messageOf(error)}`);
  if (!signedTxXdr) throw new Error('Freighter did not return signed XDR.');
  return signedTxXdr;
}

export class FreighterAdapter implements WalletAdapter {
  readonly name = 'Freighter';

  constructor(
    private readonly networkPassphrase: string = NETWORK_PASSPHRASE.TESTNET
  ) {}

  isAvailable(): boolean {
    return isFreighterInstalled();
  }

  connect(): Promise<string> {
    return connectFreighter();
  }

  sign(xdr: string, networkPassphrase: string = this.networkPassphrase): Promise<string> {
    return signTransactionWithFreighter(xdr, networkPassphrase);
  }

  disconnect(): void {
    // Freighter owns the session; nothing to clear locally.
  }
}
