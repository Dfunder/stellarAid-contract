import type { WalletAdapter } from "./types";

// Minimal subset of @lobstrco/signer-extension-api.
interface LobstrExtension {
  isConnected(): Promise<boolean>;
  getPublicKey(): Promise<string>;
  signTransaction(xdr: string): Promise<string>;
}

declare const window: Window & {
  lobstrExtension?: LobstrExtension;
};

export class LobstrAdapter implements WalletAdapter {
  readonly name = "Lobstr";

  isAvailable(): boolean {
    return (
      typeof window !== "undefined" &&
      window.lobstrExtension !== undefined
    );
  }

  async connect(): Promise<string> {
    const ext = this.extension();
    const connected = await ext.isConnected();
    if (!connected) {
      throw new Error("Lobstr Signer extension is not connected. Open the extension and unlock it.");
    }
    return ext.getPublicKey();
  }

  async sign(xdr: string): Promise<string> {
    return this.extension().signTransaction(xdr);
  }

  disconnect(): void {
    // The Lobstr extension manages its own session state.
  }

  private extension(): LobstrExtension {
    if (!this.isAvailable()) {
      throw new Error("Lobstr Signer extension is not installed or not available.");
    }
    return window.lobstrExtension!;
  }
}
