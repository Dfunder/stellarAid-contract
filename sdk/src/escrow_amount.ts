/**
 * Typed SDK binding for the escrow amount view (closes #1093).
 *
 * The contract exposes `get_escrow_amount(commission_id) -> i128`. This helper
 * keeps the method name in one place and converts the RPC string to bigint.
 */

export const GET_ESCROW_AMOUNT_METHOD = 'get_escrow_amount' as const;

export type ViewInvoker = (
  contract: string,
  method: string,
  args: readonly string[]
) => Promise<unknown>;

export class EscrowAmountValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'EscrowAmountValidationError';
  }
}

/** Read the amount escrowed for one commission, never the contract balance. */
export async function getEscrowAmount(
  invoke: ViewInvoker,
  escrowContract: string,
  commissionId: string
): Promise<bigint> {
  const value = await invoke(escrowContract, GET_ESCROW_AMOUNT_METHOD, [commissionId]);
  if (typeof value !== 'string' || !/^-?(?:0|[1-9]\d*)$/.test(value)) {
    throw new EscrowAmountValidationError(
      'get_escrow_amount must return a decimal string for an i128 amount'
    );
  }
  return BigInt(value);
}
