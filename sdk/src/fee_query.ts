// Fee breakdown query with a validated boundary (#966, #1088).

import { asBigIntString, asNumber, asObject, buildUrl, fetchJson, field, type Validator } from './net/validate.js';

export interface FeeBreakdownResult {
  grossAmount: bigint;
  platformFee: bigint;
  netAmount: bigint;
  feeBps: number;
}

const validateFeeBreakdown: Validator<FeeBreakdownResult> = (value, path) => {
  const object = asObject(value, path);
  return {
    grossAmount: field(object, 'grossAmount', path, asBigIntString),
    platformFee: field(object, 'platformFee', path, asBigIntString),
    netAmount: field(object, 'netAmount', path, asBigIntString),
    feeBps: field(object, 'feeBps', path, asNumber),
  };
};

export async function queryFeeBreakdown(
  rpcUrl: string,
  grossAmount: bigint
): Promise<FeeBreakdownResult> {
  return fetchJson(
    buildUrl(rpcUrl, ['fee-breakdown'], { amount: grossAmount.toString() }),
    validateFeeBreakdown
  );
}
