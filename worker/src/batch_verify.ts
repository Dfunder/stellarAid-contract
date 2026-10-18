// Add batch verify endpoint so operator can re-check donations in bulk (#961)
export interface VerifyResult {
  donationId: string;
  valid: boolean;
  reason: string | null;
}

export async function batchVerifyDonations(
  rpcUrl: string,
  donationIds: string[]
): Promise<VerifyResult[]> {
  const response = await fetch(rpcUrl + '/donations/batch-verify', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids: donationIds }),
  });
  if (!response.ok) {
    throw new Error('Batch verify failed: ' + response.status);
  }
  return response.json() as Promise<VerifyResult[]>;
}
