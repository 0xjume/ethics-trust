/**
 * Pro-rata share, matching `math::pro_rata_share`.
 * `floor(tokensBought * credited / creditedTotal)`.
 * Dust (`tokensBought − Σ floors`) stays in the raise ATA and is not a share.
 * Each floor's remainder is < 1 raw unit, so dust is strictly less than the
 * number of credited contributors.
 */
export const TOKEN_DECIMALS = 6;
/** 1_000_000_000 supply × 10^6. Watcher reports above this are rejected. */
export const MAX_TOKENS_BOUGHT = 1_000_000_000n * 1_000_000n;
export const MAX_DISTRIBUTE_BATCH = 5;
export const DISTRIBUTE_ATTACH_TGAS = 60;

export function proRataShare(
  tokensBought: bigint,
  credited: bigint,
  creditedTotal: bigint,
): bigint {
  if (tokensBought <= 0n || credited <= 0n || creditedTotal <= 0n) {
    return 0n;
  }
  if (tokensBought > MAX_TOKENS_BOUGHT) {
    throw new Error(`tokens bought exceeds ${MAX_TOKENS_BOUGHT.toString()}`);
  }
  return (tokensBought * credited) / creditedTotal;
}
