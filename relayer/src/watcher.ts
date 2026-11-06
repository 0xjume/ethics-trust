/**
 * The named watcher set at `create_trust` is the only
 * predecessor that may call `report_deposit` and `confirm_transfer`.
 * It is trusted for liveness and pro-rata inputs; it cannot redirect
 * destinations. Anyone may `release_pending` after ~150s (past the Solana
 * blockhash lifetime). A re-sign of the same lock supersedes earlier
 * released hashes (`pending superseded`). An honest late confirm with
 * no re-sign still books.
 *
 * After a sweep / refund / raise-refund / distribute signature lands:
 *   confirm_transfer({ message_hash: sha256(unsigned_message) hex,
 *                      solana_sig })
 * Launch uses confirm_launch_outcome(message_hash, solana_sig). The share
 * basis is the quote stored at request_launch; the watcher cannot set it.
 *
 * `watchDeposits` reads Solana balances. The long-running loop is `src/main.ts`.
 */
import {
  decideDepositReport,
  unreportedLamports,
  type DepositState,
} from "./deposit-decision.ts";
export type DepositObservation = {
  trustId: string;
  contributor: string;
  depositLamports: bigint;
  unreportedLamports: bigint;
  solanaSig: string | null;
  report: boolean;
};

export type DepositWatchRow = {
  trustId: string;
  state: DepositState;
  contributor: string;
  depositAddress: string | null;
  reported: bigint;
  swept: bigint;
  refunded: bigint;
  lastSolanaSig: string;
  signatures: readonly string[];
};

export type ConfirmTransferArgs = {
  messageHash: string;
  solanaSig: string;
};

export function confirmTransferArgs(args: ConfirmTransferArgs): {
  message_hash: string;
  solana_sig: string;
} {
  return {
    message_hash: args.messageHash,
    solana_sig: args.solanaSig,
  };
}

/**
 * Read each deposit address balance and decide whether `report_deposit`
 * should run. `signatures` are recent transactions on that address.
 * A balance with no unused signature is observed and not reported.
 */
export async function watchDeposits(
  rows: DepositWatchRow[],
  balances: { getBalance(address: string): Promise<bigint> },
): Promise<DepositObservation[]> {
  const out: DepositObservation[] = [];
  for (const row of rows) {
    const depositLamports = row.depositAddress
      ? await balances.getBalance(row.depositAddress)
      : 0n;
    const decision = decideDepositReport({
      state: row.state,
      balance: depositLamports,
      reported: row.reported,
      swept: row.swept,
      refunded: row.refunded,
      lastSolanaSig: row.lastSolanaSig,
      signatures: row.signatures,
    });
    const ahead = unreportedLamports({
      balance: depositLamports,
      reported: row.reported,
      swept: row.swept,
      refunded: row.refunded,
    });
    out.push({
      trustId: row.trustId,
      contributor: row.contributor,
      depositLamports,
      unreportedLamports: decision?.lamports ?? ahead,
      solanaSig: decision?.solanaSig ?? null,
      report: decision != null,
    });
  }
  return out;
}
