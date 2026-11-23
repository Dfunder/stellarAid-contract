/**
 * @deprecated Import from `sdk/wallets/freighter.ts` instead.
 *
 * This path is kept as a re-export only so existing consumers do not break.
 * There is exactly one implementation and one freighter-api major; the old
 * `window.freighter` shim was removed with this file's body.
 */

export {
  FREIGHTER_DOWNLOAD_URL,
  FreighterAdapter,
  FreighterNotInstalledError,
  NETWORK_PASSPHRASE,
  connectFreighter,
  isFreighterInstalled,
  signTransactionWithFreighter,
  type StellarNetwork,
  type WalletAdapter,
} from '../../../wallets/freighter.js';
