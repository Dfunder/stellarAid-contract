import type { WalletAdapter } from "./types";

// Minimal subset of @stellar/freighter-api that we depend on.
interface FreighterApi {
  isConnected(): Promise<{ isConnected: boolean }>;
  getAddress(): Promise<{ address: string }>;
  signTransaction(
    xdr: string,
    opts?: { networkPassphrase?: string }
  ): Promise<{ signedTxXdr: string }>;
}

declare const window: Window & {
  freighter?: FreighterApi;
};

export class FreighterAdapter implements WalletAdapter {
  readonly name = "Freighter";

  isAvailable(): boolean {
    return typeof window !== "undefined" && window.freighter !== undefined;
  }

  async connect(): Promise<string> {
    const api = this.api();
    const { isConnected } = await api.isConnected();
    if (!isConnected) {
      throw new Error("Freighter is not connected. Open the extension and unlock it.");
    }
    const { address } = await api.getAddress();
    return address;
  }

  async sign(xdr: string): Promise<string> {
    const { signedTxXdr } = await this.api().signTransaction(xdr);
    return signedTxXdr;
  }

  disconnect(): void {
    // Freighter manages its own session; nothing to clear on our side.
  }

  private api(): FreighterApi {
    if (!this.isAvailable()) {
      throw new Error("Freighter extension is not installed or not available.");
    }
    return window.freighter!;
  }
}
