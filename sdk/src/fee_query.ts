// Add fee breakdown query so a client can show where the platform cut lands (#966)
export interface FeeBreakdownResult {
  grossAmount: bigint;
  platformFee: bigint;
  netAmount: bigint;
  feeBps: number;
}

export async function queryFeeBreakdown(
  rpcUrl: string,
  grossAmount: bigint
): Promise<FeeBreakdownResult> {
  const response = await fetch(
    rpcUrl + '/fee-breakdown?amount=' + grossAmount.toString()
  );
  if (!response.ok) {
    throw new Error('Fee breakdown query failed: ' + response.status);
  }
  return response.json() as Promise<FeeBreakdownResult>;
}
