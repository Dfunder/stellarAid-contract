/**
 * Utility library for constructing standardized Stellar SEP-0007 payment links.
 * Reference Specification: https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0007.md
 */

interface DonationLinkParams {
  destination: string;
  amount: string;
  memo: string;
  campaignId: string;
}

/**
 * Builds a valid, URI-encoded web+stellar:pay link matching SEP-0007 specifications.
 * * @param destination Valid Stellar G... public address receiving the funds
 * @param amount String representation of asset amount to prevent floating-point precision loss
 * @param memo Explanatory text string encoded as a MEMO_TEXT type parameter
 * @param campaignId Custom tracking identifier embedded alongside operational paths
 * @returns Fully qualified web+stellar:pay deep-link scheme string
 */
export function buildDonationPaymentLink({
  destination,
  amount,
  memo,
  campaignId,
}: DonationLinkParams): string {
  // Validate basic address structural formats
  if (!destination.startsWith('G') || destination.length !== 56) {
    throw new Error('Invalid Stellar destination public key formatting configuration.');
  }

  const baseUrl = 'web+stellar:pay';
  
  const queryParams = new URLSearchParams({
    destination: destination.trim(),
    amount: amount.trim(),
    memo: memo.trim(),
    memo_type: 'MEMO_TEXT',
    msg: `Donation for Campaign ID: ${campaignId}`.trim(),
  });

  // Returns the formatted deep link protocol string
  return `${baseUrl}?${queryParams.toString()}`;
}