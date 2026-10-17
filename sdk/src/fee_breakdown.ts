// Add fee-breakdown preview to TypeScript bindings so UI can quote before submitting (#972)
export interface FeeBreakdown {
  baseFee: bigint;
  platformCut: bigint;
  networkFee: bigint;
  total: bigint;
}

export function previewFeeBreakdown(
  amount: bigint,
  platformFeeBps: number,
  networkFee: bigint
): FeeBreakdown {
  const platformCut = (amount * BigInt(platformFeeBps)) / BigInt(10000);
  const baseFee = amount - platformCut;
  const total = amount + networkFee;
  return { baseFee, platformCut, networkFee, total };
}
