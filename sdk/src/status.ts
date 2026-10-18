// Add get_status query to the SDK reporting contract deployment and version state (#971)
export interface ContractStatus {
  deployed: boolean;
  version: string;
  admin: string;
  paused: boolean;
}

export async function getContractStatus(
  contractId: string,
  rpcUrl: string
): Promise<ContractStatus> {
  const response = await fetch(rpcUrl + '/contract/' + contractId + '/status');
  if (!response.ok) {
    throw new Error('Failed to fetch contract status: ' + response.status);
  }
  return response.json() as Promise<ContractStatus>;
}
