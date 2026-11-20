// Multi-asset token registry query with a validated boundary (#968, #1088).

import { asNumber, asObject, asString, buildUrl, fetchJson, field, ValidationError, type Validator } from './net/validate.js';

export interface TokenInfo {
  assetCode: string;
  assetIssuer: string;
  contractId: string;
  decimals: number;
  name: string;
}

const validateToken: Validator<TokenInfo> = (value, path) => {
  const object = asObject(value, path);
  return {
    assetCode: field(object, 'assetCode', path, asString),
    assetIssuer: field(object, 'assetIssuer', path, asString),
    contractId: field(object, 'contractId', path, asString),
    decimals: field(object, 'decimals', path, asNumber),
    name: field(object, 'name', path, asString),
  };
};

const validateTokenList: Validator<TokenInfo[]> = (value, path) => {
  if (!Array.isArray(value)) throw new ValidationError('expected array', path);
  return value.map((token, index) => validateToken(token, `${path}[${index}]`));
};

export async function queryTokenRegistry(
  rpcUrl: string,
  assetCodes?: readonly string[]
): Promise<TokenInfo[]> {
  return fetchJson(
    buildUrl(rpcUrl, ['tokens'], { assets: assetCodes }),
    validateTokenList
  );
}
