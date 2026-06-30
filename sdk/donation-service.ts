// Define explicit SDK runtime error codes
export class DonationValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DonationValidationError';
  }
}

// 1 XLM baseline allocation mapping constant
export const MIN_DONATION_XLM = 1.0;

/**
 * Validates and triggers a donation submission transaction.
 * @throws {DonationValidationError} Forwards a user-friendly message if below limits.
 */
export const submitDonationPreflight = async (
  amountXlm: number,
  executeContractCall: () => Promise<any>
): Promise<any> => {
  // Task Requirement: Return user-friendly error message if amount is too low
  if (amountXlm < MIN_DONATION_XLM) {
    throw new DonationValidationError(
      `Donation amount must be at least ${MIN_DONATION_XLM.toFixed(2)} XLM to adequately support base network fees and wallet reserve allocations.`
    );
  }

  try {
    return await executeContractCall();
  } catch (error: any) {
    // Gracefully transform raw Soroban panics into localized text if they slide past preflight
    if (error?.message?.includes('Amount too low')) {
      throw new DonationValidationError('The transaction was rejected by the ledger: Donation amount is too low.');
    }
    throw error;
  }
};