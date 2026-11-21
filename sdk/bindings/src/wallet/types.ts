export interface WalletAdapter {
  /** Human-readable wallet name. */
  readonly name: string;

  /**
   * Connect to the wallet and return the user's public key.
   * Throws if the user rejects the connection or the wallet is unavailable.
   */
  connect(): Promise<string>;

  /**
   * Sign a base64-encoded XDR transaction envelope and return the signed XDR.
   * Throws if the user rejects the signing request.
   */
  sign(xdr: string): Promise<string>;

  /** Disconnect / clear any session state held by the adapter. */
  disconnect(): void;

  /**
   * Return true when the wallet extension or service is detectable in the
   * current environment. Does not guarantee the user is connected.
   */
  isAvailable(): boolean;
}
