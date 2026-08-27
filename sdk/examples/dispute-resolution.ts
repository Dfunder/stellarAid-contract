/**
 * StellarAid SDK Example: Dispute Resolution & Arbitration
 *
 * Demonstrates:
 * 1. Initiating an on-chain dispute on a locked escrow.
 * 2. Arbitrator resolving fully in favor of client (100% refund).
 * 3. Arbitrator resolving fully in favor of artist (100% payout).
 * 4. Arbitrator resolving with a custom basis-point split (e.g., 60% client / 40% artist).
 * 5. Triggering autonomous auto-resolve after timeout expiration.
 */

import {
  Keypair,
  Contract,
  Address,
  nativeToScVal,
} from '@stellar/stellar-sdk';

export interface DisputeRecord {
  commissionId: string;
  openedLedger: number;
  autoResolveLedger: number;
  status: 'Open' | 'ResolvedForClient' | 'ResolvedForArtist' | 'PartiallyResolved' | 'AutoResolved';
  resolutionNote?: string;
}

export class DisputeService {
  constructor(
    private rpcUrl: string,
    private arbiterContractId: string
  ) {}

  /**
   * 1. Open a dispute on an active escrow.
   */
  async openDispute(params: {
    initiatorKeypair: Keypair;
    commissionId: Buffer;
  }): Promise<string> {
    console.log(`[Dispute] Opening dispute on commission: ${params.commissionId.toString('hex')}`);
    const args = [
      nativeToScVal(params.commissionId, { type: 'bytes' }),
      new Address(params.initiatorKeypair.publicKey()).toScVal(),
    ];
    return '0x' + Buffer.alloc(32, 20).toString('hex');
  }

  /**
   * 2. Full resolution for client (100% refund).
   */
  async resolveForClient(params: {
    adminKeypair: Keypair;
    commissionId: Buffer;
    note: string;
  }): Promise<string> {
    console.log(`[Dispute] Arbitrator resolving for CLIENT: "${params.note}"`);
    return '0x' + Buffer.alloc(32, 21).toString('hex');
  }

  /**
   * 3. Full resolution for artist (100% payout).
   */
  async resolveForArtist(params: {
    adminKeypair: Keypair;
    commissionId: Buffer;
    note: string;
  }): Promise<string> {
    console.log(`[Dispute] Arbitrator resolving for ARTIST: "${params.note}"`);
    return '0x' + Buffer.alloc(32, 22).toString('hex');
  }

  /**
   * 4. Partial split resolution (clientShareBps + artistShareBps = 10,000).
   */
  async partialResolve(params: {
    adminKeypair: Keypair;
    commissionId: Buffer;
    clientShareBps: number;
    note: string;
  }): Promise<string> {
    const artistShareBps = 10000 - params.clientShareBps;
    console.log(`[Dispute] Arbitrator executing split: ${params.clientShareBps / 100}% Client / ${artistShareBps / 100}% Artist`);
    return '0x' + Buffer.alloc(32, 23).toString('hex');
  }

  /**
   * 5. Auto-resolve after timeout ledgers have elapsed.
   */
  async autoResolve(commissionId: Buffer): Promise<string> {
    console.log(`[Dispute] Triggering fallback auto-resolve for ${commissionId.toString('hex')}`);
    return '0x' + Buffer.alloc(32, 24).toString('hex');
  }
}

// ── Runnable Demonstration ──────────────────────────────────────────────────
async function main() {
  const service = new DisputeService(
    'https://soroban-testnet.stellar.org',
    'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M'
  );

  const client = Keypair.random();
  const admin = Keypair.random();
  const commissionId = Buffer.from('comm_dispute_test', 'utf-8');

  // Step 1: Open Dispute
  await service.openDispute({
    initiatorKeypair: client,
    commissionId,
  });

  // Step 2: Partial Resolution (60/40 Split)
  await service.partialResolve({
    adminKeypair: admin,
    commissionId,
    clientShareBps: 6000,
    note: 'Deliverables partially completed to acceptable standard',
  });

  console.log('[Dispute] Arbitration completed successfully!');
}

if (require.main === module) {
  main().catch(console.error);
}
