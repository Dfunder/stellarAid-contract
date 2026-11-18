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

const STELLAR_ADDRESS_RE = /^G[A-Z2-7]{55}$/;
const AMOUNT_RE = /^(?:0|[1-9]\d*)(?:\.\d+)?$/;

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

/**
 * Builds a valid, URI-encoded web+stellar:pay link matching SEP-0007 specifications.
 *
 * @param destination Valid Stellar G... public address receiving the funds
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
  const dest = destination.trim();
  const amt = amount.trim();
  const memoText = memo.trim();

  // Validate basic address structural formats
  if (!STELLAR_ADDRESS_RE.test(dest)) {
    throw new Error('Invalid Stellar destination public key formatting configuration.');
  }

  if (!AMOUNT_RE.test(amt)) {
    throw new Error('Invalid amount format.');
  }

  // SEP-0007 MEMO_TEXT is limited to 28 bytes.
  if (utf8ByteLength(memoText) > 28) {
    throw new Error('Memo exceeds SEP-0007 MEMO_TEXT 28-byte limit.');
  }

  const baseUrl = 'web+stellar:pay';

  const queryParams = new URLSearchParams({
    destination: dest,
    amount: amt,
    memo: memoText,
    memo_type: 'MEMO_TEXT',
    msg: `Donation for Campaign ID: ${campaignId}`.trim(),
  });

  // Returns the formatted deep link protocol string
  return `${baseUrl}?${queryParams.toString()}`;
}
