// Add escrow partial-settlement view so operator can see unbonded remainder (#964)
export interface EscrowPartialSettlement {
  escrowId: string;
  totalAmount: bigint;
  settledAmount: bigint;
  unbondedRemainder: bigint;
  settlementCount: number;
  isFullySettled: boolean;
}

export async function getEscrowPartialSettlement(
  rpcUrl: string,
  escrowId: string
): Promise<EscrowPartialSettlement> {
  const response = await fetch(rpcUrl + '/escrow/' + escrowId + '/settlement');
  if (!response.ok) {
    throw new Error('Escrow settlement query failed: ' + response.status);
  }
  return response.json() as Promise<EscrowPartialSettlement>;
}
