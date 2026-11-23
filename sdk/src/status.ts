// Contract status query with a validated boundary (#971, #1088).

import { asBoolean, asObject, asString, buildUrl, fetchJson, field, type Validator } from './net/validate.js';

export interface ContractStatus {
  deployed: boolean;
  version: string;
  admin: string;
  paused: boolean;
}

const validateContractStatus: Validator<ContractStatus> = (value, path) => {
  const object = asObject(value, path);
  return {
    deployed: field(object, 'deployed', path, asBoolean),
    version: field(object, 'version', path, asString),
    admin: field(object, 'admin', path, asString),
    paused: field(object, 'paused', path, asBoolean),
  };
};

export async function getContractStatus(
  contractId: string,
  rpcUrl: string
): Promise<ContractStatus> {
  return fetchJson(
    buildUrl(rpcUrl, ['contract', contractId, 'status']),
    validateContractStatus
  );
}
