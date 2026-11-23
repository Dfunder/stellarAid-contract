// Escrow partial-settlement view with a validated boundary (#964, #1088).

import { asBigIntString, asBoolean, asNumber, asObject, asString, buildUrl, fetchJson, field, type Validator } from './net/validate.js';

export interface EscrowPartialSettlement {
  escrowId: string;
  totalAmount: bigint;
  settledAmount: bigint;
  unbondedRemainder: bigint;
  settlementCount: number;
  isFullySettled: boolean;
}

const validateSettlement: Validator<EscrowPartialSettlement> = (value, path) => {
  const object = asObject(value, path);
  return {
    escrowId: field(object, 'escrowId', path, asString),
    totalAmount: field(object, 'totalAmount', path, asBigIntString),
    settledAmount: field(object, 'settledAmount', path, asBigIntString),
    unbondedRemainder: field(object, 'unbondedRemainder', path, asBigIntString),
    settlementCount: field(object, 'settlementCount', path, asNumber),
    isFullySettled: field(object, 'isFullySettled', path, asBoolean),
  };
};

export async function getEscrowPartialSettlement(
  rpcUrl: string,
  escrowId: string
): Promise<EscrowPartialSettlement> {
  return fetchJson(
    buildUrl(rpcUrl, ['escrow', escrowId, 'settlement']),
    validateSettlement
  );
}
