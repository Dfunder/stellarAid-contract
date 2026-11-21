// Shared types for the contract-interaction bindings (issue #866).
//
// These sit on top of the wallet adapters: an adapter signs, a ContractClient
// turns a plain method name + arguments into a Soroban invocation, and
// everything here describes the network that invocation runs on.

import type { WalletAdapter } from "../wallet/types";

export type Network = "testnet" | "mainnet" | "futurenet" | "standalone";

export interface NetworkConfig {
  network: Network;
  rpcUrl: string;
  horizonUrl: string;
  networkPassphrase: string;
}

export const TESTNET: NetworkConfig = {
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  horizonUrl: "https://horizon-testnet.stellar.org",
  networkPassphrase: "Test SDF Network ; September 2015",
};

export const PUBLIC: NetworkConfig = {
  network: "mainnet",
  rpcUrl: "https://soroban-rpc.stellar.org",
  horizonUrl: "https://horizon.stellar.org",
  networkPassphrase: "Public Global Stellar Network ; September 2015",
};

/** A Stellar address: account (`G...`) or contract (`C...`). */
export type ScAddress = string;

export interface ContractClientConfig {
  /** Deployed contract id (`C...`). */
  contractId: ScAddress;
  network: NetworkConfig;
  /** The account the transactions are built from. */
  source: ScAddress;
  /** Wallet adapter used to sign envelopes before submission. */
  wallet: WalletAdapter;
}

/** One decoded Soroban event. */
export interface ParsedEvent {
  topic: unknown[];
  data: unknown;
}

/** What a successful invocation produced. */
export interface InvocationResult<T> {
  value: T;
  /** Events emitted by the invocation, decoded best-effort. */
  events: ParsedEvent[];
  /** Resource cost reported by the simulation, when available. */
  cost?: unknown;
}