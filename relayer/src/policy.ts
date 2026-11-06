/**
 * Sweep and refund amounts the relayer is allowed to put in a message.
 * The contract still checks the shape. These helpers only avoid building
 * a transfer Solana would reject for rent, or a partial refund the
 * contract would reject.
 */
import { fillFeeLamports } from "./launch-math.ts";

/** Solana system-account rent-exempt minimum. Matches the contract. */
export const RENT_EXEMPT_MIN = 890_880n;

export { fillFeeLamports };

/**
 * Largest amount we can move without leaving the source in the
 * `0 < leftover < rent` band. A full close (leftover 0) is allowed.
 * Returns null when the desired amount cannot be sent, even in part.
 * Callers that must move an exact refund compare the result to `desired`.
 */
export function safeTransferAmount(balance: bigint, desired: bigint): bigint | null {
  if (desired <= 0n || balance <= 0n) return null;
  const amount = desired < balance ? desired : balance;
  const leftover = balance - amount;
  if (leftover === 0n || leftover >= RENT_EXEMPT_MIN) return amount;
  if (balance <= RENT_EXEMPT_MIN) {
    return desired >= balance ? balance : null;
  }
  const room = balance - RENT_EXEMPT_MIN;
  if (room <= 0n || room > desired) return null;
  return room;
}

/** Lamports the contract still considers sitting on the deposit address. */
export function bookedDepositBalance(args: {
  reported: bigint;
  swept: bigint;
  refunded: bigint;
}): bigint {
  const remaining = args.reported - args.swept - args.refunded;
  return remaining > 0n ? remaining : 0n;
}
