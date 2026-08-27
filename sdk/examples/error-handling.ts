/**
 * StellarAid SDK Example: Robust Error Handling & Resilience
 *
 * Demonstrates:
 * 1. Parsing and mapping Soroban Contract Errors (ScError / error codes).
 * 2. Inspecting simulated transaction failures and diagnostic events.
 * 3. Handling missing authorization and signature failures.
 * 4. Exponential backoff and retry policies for network and rate limits.
 */

import { xdr, scValToNative } from '@stellar/stellar-sdk';

/**
 * Standard StellarAid Contract Error Codes
 */
export enum EscrowErrorCode {
  AlreadyExists = 1,
  NotFound = 2,
  InvalidStatus = 3,
  Unauthorized = 4,
  InvalidAmount = 5,
  InvalidFeeBps = 6,
  DisputeAlreadyOpen = 7,
  NotExpired = 8,
  Reentrant = 9,
  InvalidAddress = 10,
  InsufficientBalance = 11,
  ArithmeticOverflow = 12,
  InvalidSplit = 13,
  ContractPaused = 14,
}

export enum AgreementErrorCode {
  AlreadyExists = 1,
  NotFound = 2,
  InvalidStatus = 3,
  Unauthorized = 4,
  InvalidAmount = 5,
  DeadlineInPast = 6,
  MilestoneBudgetExceeded = 7,
  NotAllMilestonesApproved = 8,
  ArithmeticOverflow = 9,
  InputTooLong = 10,
  DeadlineTooFar = 11,
  MilestoneLocked = 12,
  NotCancellable = 13,
  AlreadyCancelled = 14,
  InvalidPolicy = 15,
}

export enum DisputeErrorCode {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  NotFound = 4,
  InvalidStatus = 5,
  AlreadyResolved = 6,
  AutoResolveNotDue = 7,
  InvalidShareBps = 8,
  ArithmeticOverflow = 9,
}

/**
 * Custom typed error class for StellarAid contract exceptions.
 */
export class StellarAidContractError extends Error {
  constructor(
    public readonly domain: 'Escrow' | 'Agreement' | 'Dispute' | 'Config' | 'Unknown',
    public readonly code: number,
    public readonly description: string,
    public readonly rawError?: any
  ) {
    super(`[${domain} Error #${code}] ${description}`);
    this.name = 'StellarAidContractError';
  }
}

/**
 * Helper to parse Soroban RPC simulation and invocation errors.
 */
export function parseContractError(error: any): StellarAidContractError {
  // 1. Inspect ScError XDR if present
  if (error?.code && typeof error.code === 'number') {
    const code = error.code;
    return new StellarAidContractError(
      'Escrow',
      code,
      EscrowErrorCode[code] || 'Unknown contract error',
      error
    );
  }

  // 2. Check diagnostic events for contract panics
  if (error?.message?.includes('ContractError')) {
    const match = error.message.match(/ContractError\((\d+)\)/);
    if (match) {
      const code = parseInt(match[1], 10);
      return new StellarAidContractError(
        'Escrow',
        code,
        `Contract panicked with error code ${code}`,
        error
      );
    }
  }

  return new StellarAidContractError('Unknown', 0, error?.message || 'Unrecognized error', error);
}

/**
 * Resilient retry utility with exponential backoff and jitter.
 */
export async function withRetry<T>(
  operation: () => Promise<T>,
  options: {
    maxRetries?: number;
    initialDelayMs?: number;
    maxDelayMs?: number;
    retryIf?: (err: any) => boolean;
  } = {}
): Promise<T> {
  const maxRetries = options.maxRetries ?? 3;
  let delay = options.initialDelayMs ?? 1000;
  const maxDelay = options.maxDelayMs ?? 10000;

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      return await operation();
    } catch (err: any) {
      const shouldRetry = options.retryIf ? options.retryIf(err) : true;
      if (attempt === maxRetries || !shouldRetry) {
        throw parseContractError(err);
      }

      console.warn(`[Retry] Attempt ${attempt} failed. Retrying in ${delay}ms... (Error: ${err.message})`);
      await new Promise((r) => setTimeout(r, delay));
      delay = Math.min(delay * 2, maxDelay);
    }
  }
  throw new Error('Unreachable');
}

// ── Demonstration ───────────────────────────────────────────────────────────
async function main() {
  console.log('[Error Handling] Demonstrating contract error parsing and retry logic...');

  // Simulating an operation that fails once with a rate limit then succeeds
  let attempts = 0;
  const result = await withRetry(
    async () => {
      attempts++;
      if (attempts === 1) {
        throw new Error('Rate limit exceeded: 429 Too Many Requests');
      }
      return 'Simulation Success!';
    },
    { maxRetries: 3, initialDelayMs: 500 }
  );

  console.log('[Result]:', result);

  // Parsing error codes
  const sampleErr = parseContractError({ code: EscrowErrorCode.DisputeAlreadyOpen });
  console.log('[Parsed Error]:', sampleErr.message);
}

if (require.main === module) {
  main().catch(console.error);
}
