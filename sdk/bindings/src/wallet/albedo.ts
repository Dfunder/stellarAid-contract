import type { WalletAdapter } from "./types";

// Minimal subset of the albedo-link API surface.
interface AlbedoLib {
  publicKey(params?: { token?: string }): Promise<{ pubkey: string }>;
  tx(params: {
    xdr: string;
    network?: string;
    submit?: boolean;
  }): Promise<{ signed_envelope_xdr: string }>;
}

declare const window: Window & { albedo?: AlbedoLib };

// albedo-link is also available as an ES module default export.
let _albedo: AlbedoLib | undefined;

async function getAlbedo(): Promise<AlbedoLib> {
  if (_albedo) return _albedo;
  if (typeof window !== "undefined" && window.albedo) {
    _albedo = window.albedo;
    return _albedo;
  }
  try {
    // Indirect specifier prevents TypeScript from statically resolving the
    // optional peer dependency, which may not be installed in all environments.
    const specifier = "albedo-link";
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const mod = await (Function("s", "return import(s)")(specifier) as Promise<any>);
    _albedo = (mod.default ?? mod) as AlbedoLib;
    return _albedo;
  } catch {
    throw new Error("albedo-link is not installed. Run: npm install albedo-link");
  }
}

export class AlbedoAdapter implements WalletAdapter {
  readonly name = "Albedo";

  private _publicKey: string | null = null;

  isAvailable(): boolean {
    // Albedo works via a popup — no extension required. Always available in a browser.
    return typeof window !== "undefined";
  }

  async connect(): Promise<string> {
    const albedo = await getAlbedo();
    const { pubkey } = await albedo.publicKey();
    this._publicKey = pubkey;
    return pubkey;
  }

  async sign(xdr: string): Promise<string> {
    const albedo = await getAlbedo();
    const { signed_envelope_xdr } = await albedo.tx({ xdr, submit: false });
    return signed_envelope_xdr;
  }

  disconnect(): void {
    this._publicKey = null;
  }
}
