/**
 * Whether the watcher should call `report_deposit`.
 *
 * The contract adds `lamports` to the row. It does not set the balance.
 * On-chain SOL left on the deposit address should equal
 * `reported - swept - refunded`. Anything above that is a new deposit.
 * `report_deposit` also requires a real Solana signature (64–128 chars)
 * that has not been used. This module never invents one.
 */
import { bookedDepositBalance } from "./policy.ts";

export type DepositState =
  | "Raising"
  | "Filled"
  | "Executing"
  | "Launched"
  | "Distributed"
  | "Refunding";

const CLOSED: ReadonlySet<DepositState> = new Set(["Launched", "Distributed"]);

export type DepositDecision = {
  lamports: bigint;
  solanaSig: string;
};

export function unreportedLamports(args: {
  balance: bigint;
  reported: bigint;
  swept: bigint;
  refunded: bigint;
}): bigint {
  if (args.balance <= 0n) return 0n;
  const booked = bookedDepositBalance(args);
  return args.balance > booked ? args.balance - booked : 0n;
}

export function usableDepositSignature(
  signatures: readonly string[],
  lastSolanaSig: string,
  burned: ReadonlySet<string> = new Set(),
): string | null {
  for (const sig of signatures) {
    if (sig.length < 64 || sig.length > 128) continue;
    if (sig === lastSolanaSig) continue;
    if (burned.has(sig)) continue;
    return sig;
  }
  return null;
}

export function decideDepositReport(args: {
  state: DepositState;
  balance: bigint;
  reported: bigint;
  swept: bigint;
  refunded: bigint;
  lastSolanaSig: string;
  signatures: readonly string[];
  burned?: ReadonlySet<string>;
}): DepositDecision | null {
  if (CLOSED.has(args.state)) return null;
  const lamports = unreportedLamports(args);
  if (lamports <= 0n) return null;
  const solanaSig = usableDepositSignature(
    args.signatures,
    args.lastSolanaSig,
    args.burned,
  );
  if (!solanaSig) return null;
  return { lamports, solanaSig };
}
