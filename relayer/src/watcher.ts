/**
 * Watcher stub. The named watcher set at `create_trust` is the only
 * predecessor that may call `report_deposit` and `confirm_transfer`.
 * It is trusted for liveness and pro-rata inputs; it cannot redirect
 * destinations. Anyone may `release_pending` after ~90s.
 *
 * After a sweep / refund / raise-refund signature lands on Solana:
 *   confirm_transfer({ message_hash: sha256(unsigned_message) hex,
 *                      solana_sig })
 */
export type SweepReport = {
  trustId: string;
  contributor: string;
  sweptLamports: bigint;
  depositLamports: bigint;
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

export async function watchDeposits(_rpcUrl: string): Promise<SweepReport[]> {
  return [];
}
