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

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
}

export function computeCampaignProgress(
  targetAmount: bigint,
  raisedAmount: bigint,
  deadlineMs: number,
  campaignId: string,
  now: number = Date.now()
): CampaignProgress {
  const safeRaised = raisedAmount < 0n ? 0n : raisedAmount;
  const safeTarget = targetAmount < 0n ? 0n : targetAmount;
  const remaining =
    safeTarget > safeRaised ? safeTarget - safeRaised : 0n;

  let percent: number;
  if (safeTarget === 0n) {
    percent = safeRaised > 0n ? 100 : 0;
  } else {
    // Compute in integer basis points-equivalent scaled steps to avoid float issues for typical values,
    // then convert to a clamped percentage.
    const pctTimes100 = (safeRaised * 10000n) / safeTarget; // percent * 100
    percent = Number(pctTimes100) / 100;
  }

  const currentTime = Number.isFinite(now) ? now : Date.now();

  return {
    campaignId,
    targetAmount: safeTarget,
    raisedAmount: safeRaised,
    remainingAmount: remaining,
    deadlineMs,
    isExpired: currentTime > deadlineMs,
    percentComplete: clampPercent(percent),
  };
}
