// Add dispute timeline view so case history is readable on-chain (#965)
export interface DisputeEvent {
  timestamp: number;
  eventType: string;
  actor: string;
  details: string;
}

export interface DisputeTimeline {
  disputeId: string;
  events: DisputeEvent[];
  currentStatus: string;
  openedAtMs: number;
  resolvedAtMs: number | null;
}

export async function getDisputeTimeline(
  rpcUrl: string,
  disputeId: string
): Promise<DisputeTimeline> {
  const response = await fetch(rpcUrl + '/dispute/' + disputeId + '/timeline');
  if (!response.ok) {
    throw new Error('Dispute timeline query failed: ' + response.status);
  }
  return response.json() as Promise<DisputeTimeline>;
}
