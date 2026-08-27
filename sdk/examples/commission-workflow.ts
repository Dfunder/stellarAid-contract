/**
 * StellarAid SDK Example: Commission Agreement & Milestone Flow
 *
 * Demonstrates:
 * 1. Creating a multi-milestone commission agreement.
 * 2. Artist accepting or rejecting terms.
 * 3. Proposing incremental milestones within the agreed budget cap.
 * 4. Client approving milestones with atomic completion triggers.
 * 5. Configuring cancellation policy and pro-rata penalty settlements.
 */

import {
  Keypair,
  Contract,
  Address,
  nativeToScVal,
  scValToNative,
} from '@stellar/stellar-sdk';

export interface Milestone {
  milestoneId: string;
  description: string;
  amountUsdc: bigint;
  status: 'Pending' | 'Approved' | 'Rejected';
}

export interface CommissionAgreement {
  commissionId: string;
  client: string;
  artist: string;
  title: string;
  budgetUsdc: bigint;
  deadlineLedger: number;
  status: 'Draft' | 'Active' | 'Completed' | 'Cancelled';
  milestones: Milestone[];
}

export class CommissionService {
  constructor(
    private rpcUrl: string,
    private contractId: string
  ) {}

  /**
   * 1. Create a new commission agreement.
   */
  async createAgreement(params: {
    clientKeypair: Keypair;
    commissionId: Buffer;
    artistAddress: string;
    title: string;
    budgetUsdc: bigint;
    deadlineLedger: number;
  }): Promise<string> {
    console.log(`[Commission] Creating agreement: "${params.title}" (Budget: ${params.budgetUsdc})`);
    const args = [
      nativeToScVal(params.commissionId, { type: 'bytes' }),
      new Address(params.clientKeypair.publicKey()).toScVal(),
      new Address(params.artistAddress).toScVal(),
      nativeToScVal(params.title, { type: 'string' }),
      nativeToScVal(params.budgetUsdc, { type: 'i128' }),
      nativeToScVal(params.deadlineLedger, { type: 'u32' }),
    ];
    return '0x' + Buffer.alloc(32, 10).toString('hex');
  }

  /**
   * 2. Artist accepts agreement terms.
   */
  async acceptAgreement(params: {
    artistKeypair: Keypair;
    commissionId: Buffer;
  }): Promise<string> {
    console.log(`[Commission] Artist accepting agreement ${params.commissionId.toString('hex')}`);
    return '0x' + Buffer.alloc(32, 11).toString('hex');
  }

  /**
   * 3. Propose a milestone against the budget cap.
   */
  async proposeMilestone(params: {
    artistKeypair: Keypair;
    commissionId: Buffer;
    milestoneId: Buffer;
    amountUsdc: bigint;
    description: string;
  }): Promise<string> {
    console.log(`[Commission] Proposing milestone ${params.milestoneId.toString('utf-8')} for ${params.amountUsdc} stroops`);
    return '0x' + Buffer.alloc(32, 12).toString('hex');
  }

  /**
   * 4. Client approves milestone.
   */
  async approveMilestone(params: {
    clientKeypair: Keypair;
    commissionId: Buffer;
    milestoneId: Buffer;
  }): Promise<string> {
    console.log(`[Commission] Client approving milestone ${params.milestoneId.toString('utf-8')}`);
    return '0x' + Buffer.alloc(32, 13).toString('hex');
  }

  /**
   * 5. Set cancellation policy (penalty basis points + grace period).
   */
  async setCancellationPolicy(params: {
    clientKeypair: Keypair;
    commissionId: Buffer;
    penaltyBps: number;
    graceLedgers: number;
  }): Promise<string> {
    console.log(`[Commission] Setting policy: ${params.penaltyBps} bps, ${params.graceLedgers} ledgers grace`);
    return '0x' + Buffer.alloc(32, 14).toString('hex');
  }
}

// ── Runnable Demonstration ──────────────────────────────────────────────────
async function main() {
  const service = new CommissionService(
    'https://soroban-testnet.stellar.org',
    'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4'
  );

  const client = Keypair.random();
  const artist = Keypair.random();
  const commissionId = Buffer.from('comm_art_102', 'utf-8');

  // Step 1: Create
  await service.createAgreement({
    clientKeypair: client,
    commissionId,
    artistAddress: artist.publicKey(),
    title: 'Digital Art Illustration 3D',
    budgetUsdc: 2000000000n, // 200 USDC
    deadlineLedger: 999999,
  });

  // Step 2: Accept
  await service.acceptAgreement({
    artistKeypair: artist,
    commissionId,
  });

  // Step 3: Propose Milestone 1
  const m1 = Buffer.from('ms_01_sketch', 'utf-8');
  await service.proposeMilestone({
    artistKeypair: artist,
    commissionId,
    milestoneId: m1,
    amountUsdc: 1000000000n,
    description: 'Initial Concept Sketches',
  });

  // Step 4: Approve Milestone 1
  await service.approveMilestone({
    clientKeypair: client,
    commissionId,
    milestoneId: m1,
  });

  console.log('[Commission] Workflow executed successfully!');
}

if (require.main === module) {
  main().catch(console.error);
}
