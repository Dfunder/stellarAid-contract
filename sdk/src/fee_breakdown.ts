// Fee-breakdown preview shared with the validated SDK boundary (#972, #1088).

import { ValidationError } from './net/validate.js';

export { ValidationError } from './net/validate.js';

export interface FeeBreakdown {
  baseFee: bigint;
  platformCut: bigint;
  networkFee: bigint;
  /** Total charged: baseFee + platformCut + networkFee (and baseFee + platformCut === amount). */
  total: bigint;
}

export function previewFeeBreakdown(
  amount: bigint,
  platformFeeBps: number,
  networkFee: bigint
): FeeBreakdown {
  if (!Number.isInteger(platformFeeBps) || platformFeeBps < 0 || platformFeeBps > 10_000) {
    throw new ValidationError('platformFeeBps must be an integer between 0 and 10000');
  }
  if (amount < 0n) {
    throw new ValidationError('amount must be non-negative');
  }
  if (networkFee < 0n) {
    throw new ValidationError('networkFee must be non-negative');
  }

  const platformCut = (amount * BigInt(platformFeeBps)) / 10_000n;
  const baseFee = amount - platformCut;
  const total = baseFee + platformCut + networkFee;

  return { baseFee, platformCut, networkFee, total };
}
