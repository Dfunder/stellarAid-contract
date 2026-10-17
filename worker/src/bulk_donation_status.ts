// Add bulk donation status endpoint to worker for dashboard use case (#962)
export interface DonationStatus {
  donationId: string;
  status: 'pending' | 'confirmed' | 'failed' | 'refunded';
  amount: bigint;
  campaignId: string;
  donor: string;
  confirmedAtMs: number | null;
}

export async function getBulkDonationStatus(
  rpcUrl: string,
  donationIds: string[]
): Promise<DonationStatus[]> {
  const ids = donationIds.join(',');
  const response = await fetch(rpcUrl + '/donations/bulk?ids=' + ids);
  if (!response.ok) {
    throw new Error('Bulk donation status failed: ' + response.status);
  }
  return response.json() as Promise<DonationStatus[]>;
}
