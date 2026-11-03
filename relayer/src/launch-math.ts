/** 1.5% fill fee (150 bps) taken from credited raise before the first buy. */
export const DEFAULT_FILL_FEE_BPS = 150;

export function fillFeeLamports(
  credited: bigint,
  fillFeeBps: number = DEFAULT_FILL_FEE_BPS,
): bigint {
  if (credited <= 0n || fillFeeBps <= 0) {
    return 0n;
  }
  return (credited * BigInt(fillFeeBps)) / 10_000n;
}

export function firstBuyLamports(
  credited: bigint,
  fillFeeBps: number = DEFAULT_FILL_FEE_BPS,
): bigint {
  const fee = fillFeeLamports(credited, fillFeeBps);
  if (credited <= fee) {
    return 0n;
  }
  return credited - fee;
}

/** Same pin as the contract: hex(sha256(config \\0 name \\0 symbol \\0 uri)). */
export async function pinParamsHash(
  dbcConfig: string,
  name: string,
  symbol: string,
  uri: string,
): Promise<string> {
  const { createHash } = await import("node:crypto");
  return createHash("sha256")
    .update(`${dbcConfig}\0${name}\0${symbol}\0${uri}`)
    .digest("hex");
}
