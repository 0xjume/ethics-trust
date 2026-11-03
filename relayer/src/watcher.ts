/**
 * Permissionless watcher stub. A later PR reads Solana balances and reports
 * them to the NEAR contract. The watcher is trusted for liveness and pro-rata
 * inputs; it cannot redirect funds.
 */
export type SweepReport = {
  trustId: string;
  contributor: string;
  sweptLamports: bigint;
  depositLamports: bigint;
};

export async function watchDeposits(_rpcUrl: string): Promise<SweepReport[]> {
  return [];
}
