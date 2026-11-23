// Typed bindings for the core StellarAid contracts (issue #866).
//
// Each binding is a thin wrapper over `ContractClient` that pins the method
// names and argument shapes of the on-chain function signatures, so callers
// get editor-completion and type checking over their contract interactions
// instead of passing magic strings around. The signatures below mirror the
// Rust contracts in `contracts/`.

import { ContractClient, type Simulation } from "./client";
import type { ContractClientConfig, ScAddress } from "./types";

export type Status = "Locked" | "Released" | "Refunded" | "Disputed" | "Expired" | "Cancelled";

export interface EscrowRecord {
  commission_id: ScAddress;
  client: ScAddress;
  artist: ScAddress;
  amount: bigint;
  status: Status;
  created_ledger: number;
}

/** Type of an escrow invocation from `contracts/escrow`. */
export class EscrowClient {
  constructor(private readonly client: ContractClient) {}

  static async for(config: ContractClientConfig): Promise<EscrowClient> {
    return new EscrowClient(await ContractClient.for(config));
  }

  simulateCreateEscrow(commissionId: string, client: ScAddress, artist: ScAddress, amount: number): Promise<Simulation> {
    return this.client.simulate("create_escrow", [commissionId, client, artist, { type: "i128", value: amount }]);
  }

  simulateOpenDispute(commissionId: string, by: ScAddress): Promise<Simulation> {
    return this.client.simulate("open_dispute", [commissionId, by]);
  }

  simulateRelease(commissionId: string, caller: ScAddress): Promise<Simulation> {
    return this.client.simulate("release_payment", [commissionId, caller]);
  }

  simulateRefund(commissionId: string, caller: ScAddress): Promise<Simulation> {
    return this.client.simulate("refund_client", [commissionId, caller]);
  }

  /** Read-only: fetch an escrow record. */
  async getEscrow(commissionId: string): Promise<EscrowRecord | undefined> {
    const simulation = await this.client.simulate("get_escrow", [commissionId]);
    return simulation.result as EscrowRecord | undefined;
  }
}

export interface AgreementRecord {
  commission_id: ScAddress;
  client: ScAddress;
  artist: ScAddress;
  title: string;
  budget_usdc: bigint;
  deadline_ledger: number;
  status: string;
}

/** Type of a commission-agreement invocation from `contracts/commission_agreement`. */
export class CommissionClient {
  constructor(private readonly client: ContractClient) {}

  static async for(config: ContractClientConfig): Promise<CommissionClient> {
    return new CommissionClient(await ContractClient.for(config));
  }

  simulateCreateAgreement(commissionId: string, client: ScAddress, artist: ScAddress, title: string, budgetUsdc: number, deadlineLedger: number): Promise<Simulation> {
    return this.client.simulate("create_agreement", [
      commissionId,
      client,
      artist,
      title,
      { type: "i128", value: budgetUsdc },
      deadlineLedger,
    ]);
  }

  simulateAcceptAgreement(commissionId: string): Promise<Simulation> {
    return this.client.simulate("accept_agreement", [commissionId]);
  }

  simulateApproveMilestone(commissionId: string, milestoneId: string): Promise<Simulation> {
    return this.client.simulate("approve_milestone", [commissionId, milestoneId]);
  }

  async getAgreement(commissionId: string): Promise<AgreementRecord | undefined> {
    const simulation = await this.client.simulate("get_agreement", [commissionId]);
    return simulation.result as AgreementRecord | undefined;
  }
}

export interface DisputeRecord {
  commission_id: ScAddress;
  client: ScAddress;
  artist: ScAddress;
  status: string;
}

/** Type of a dispute invocation from `contracts/dispute_arbiter`. */
export class DisputeClient {
  constructor(private readonly client: ContractClient) {}

  static async for(config: ContractClientConfig): Promise<DisputeClient> {
    return new DisputeClient(await ContractClient.for(config));
  }

  simulateOpen(commissionId: string, openedBy: ScAddress): Promise<Simulation> {
    return this.client.simulate("open_dispute", [commissionId, openedBy]);
  }

  simulateResolve(commissionId: string, clientAmount: number, artistAmount: number): Promise<Simulation> {
    return this.client.simulate("resolve_dispute", [
      commissionId,
      { type: "i128", value: clientAmount },
      { type: "i128", value: artistAmount },
    ]);
  }

  async getDispute(commissionId: string): Promise<DisputeRecord | undefined> {
    const simulation = await this.client.simulate("get_dispute", [commissionId]);
    return simulation.result as DisputeRecord | undefined;
  }
}

export interface CampaignRecord {
  id: ScAddress;
  organizer: ScAddress;
  title: string;
  goal: bigint;
  status: string;
}

/** Type of a campaign/donation invocation from `contracts/campaign`. */
export class CampaignClient {
  constructor(private readonly client: ContractClient) {}

  static async for(config: ContractClientConfig): Promise<CampaignClient> {
    return new CampaignClient(await ContractClient.for(config));
  }

  simulateCreateCampaign(organizer: ScAddress, title: string, goal: number, deadlineLedger: number): Promise<Simulation> {
    return this.client.simulate("create_campaign", [organizer, title, { type: "i128", value: goal }, deadlineLedger]);
  }

  simulateDonate(donor: ScAddress, campaignId: string, amount: number, anonymous: boolean): Promise<Simulation> {
    return this.client.simulate("donate", [donor, campaignId, { type: "i128", value: amount }, undefined, anonymous]);
  }

  async getCampaign(campaignId: string): Promise<CampaignRecord | undefined> {
    const simulation = await this.client.simulate("get_campaign", [campaignId]);
    return simulation.result as CampaignRecord | undefined;
  }
}