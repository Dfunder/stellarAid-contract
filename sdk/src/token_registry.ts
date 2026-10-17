// Add multi-asset token registry query to the SDK (#968)
export interface TokenInfo {
  assetCode: string;
  assetIssuer: string;
  contractId: string;
  decimals: number;
  name: string;
}

export async function queryTokenRegistry(
  rpcUrl: string,
  assetCodes?: string[]
): Promise<TokenInfo[]> {
  const params = assetCodes ? '?assets=' + assetCodes.join(',') : '';
  const response = await fetch(rpcUrl + '/tokens' + params);
  if (!response.ok) {
    throw new Error('Token registry query failed: ' + response.status);
  }
  return response.json() as Promise<TokenInfo[]>;
}
