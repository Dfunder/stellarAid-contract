// Add campaign funding-progress query with deadline and remaining target (#967)
export interface CampaignProgress {
  campaignId: string;
  targetAmount: bigint;
  raisedAmount: bigint;
  remainingAmount: bigint;
  deadlineMs: number;
  isExpired: boolean;
  percentComplete: number;
}

export function computeCampaignProgress(
  targetAmount: bigint,
  raisedAmount: bigint,
  deadlineMs: number
): CampaignProgress {
  const remaining = targetAmount > raisedAmount ? targetAmount - raisedAmount : 0n;
  const pct = Number((raisedAmount * 100n) / targetAmount);
  return {
    campaignId: '',
    targetAmount,
    raisedAmount,
    remainingAmount: remaining,
    deadlineMs,
    isExpired: Date.now() > deadlineMs,
    percentComplete: pct,
  };
}
