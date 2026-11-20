/**
 * Donation preflight with a generic result type and a typed error union
 * (closes #1089).
 *
 * The contract is the authority on minimum amounts. There is deliberately no
 * hard-coded XLM floor here: a caller may pass a per-asset minimum as a UX
 * hint, and USDC donations are never measured against an XLM constant.
 */

export type DonationAsset = 'XLM' | 'USDC';

export type DonationValidationCode =
  | 'INVALID_AMOUNT'
  | 'AMOUNT_TOO_LOW'
  | 'INVALID_ASSET';

export class DonationValidationError extends Error {
  readonly code: DonationValidationCode;

  constructor(code: DonationValidationCode, message: string) {
    super(message);
    this.name = 'DonationValidationError';
    this.code = code;
  }
}

/** Discriminated union for anything the ledger or wallet can reject with. */
export type SorobanRejectionError =
  | { readonly kind: 'amount_too_low'; readonly detail: string }
  | { readonly kind: 'invalid_asset'; readonly detail: string }
  | { readonly kind: 'contract_rejected'; readonly detail: string }
  | { readonly kind: 'unknown'; readonly cause: unknown };

const AMOUNT_TOO_LOW = /amount\s+too\s+low/i;
const INVALID_ASSET = /invalid\s+asset|unsupported\s+asset/i;

/** Narrow an `unknown` thrown value without `any` or loose property access. */
export function classifySorobanRejection(error: unknown): SorobanRejectionError {
  if (error instanceof Error) {
    if (AMOUNT_TOO_LOW.test(error.message)) {
      return { kind: 'amount_too_low', detail: error.message };
    }
    if (INVALID_ASSET.test(error.message)) {
      return { kind: 'invalid_asset', detail: error.message };
    }
    return { kind: 'contract_rejected', detail: error.message };
  }
  return { kind: 'unknown', cause: error };
}

/**
 * Optional per-asset minimums, read from contract config by the caller.
 * These are UX hints only; the ledger still decides what it accepts.
 */
export interface DonationLimits {
  readonly minimumByAsset?: Partial<Record<DonationAsset, bigint>>;
}

export function donationMinimumHint(
  limits: DonationLimits,
  asset: DonationAsset
): bigint | undefined {
  return limits.minimumByAsset?.[asset];
}

/**
 * Validate a donation and run the contract call.
 *
 * @returns exactly whatever `executeContractCall` returns, typed as `T`.
 * @throws {DonationValidationError} for a low amount or a mapped ledger rejection.
 */
export async function submitDonationPreflight<T>(
  amount: bigint,
  asset: DonationAsset,
  executeContractCall: () => Promise<T>,
  limits: DonationLimits = {}
): Promise<T> {
  if (amount <= 0n) {
    throw new DonationValidationError('INVALID_AMOUNT', 'Donation amount must be positive.');
  }

  const minimum = donationMinimumHint(limits, asset);
  if (minimum !== undefined && amount < minimum) {
    throw new DonationValidationError(
      'AMOUNT_TOO_LOW',
      `Donation amount must be at least ${minimum.toString()} ${asset} base units.`
    );
  }

  try {
    return await executeContractCall();
  } catch (error: unknown) {
    const rejection = classifySorobanRejection(error);
    if (rejection.kind === 'amount_too_low') {
      throw new DonationValidationError(
        'AMOUNT_TOO_LOW',
        'The ledger rejected this donation: the amount is too low.'
      );
    }
    if (rejection.kind === 'invalid_asset') {
      throw new DonationValidationError(
        'INVALID_ASSET',
        'The ledger does not accept this donation asset.'
      );
    }
    throw error;
  }
}
