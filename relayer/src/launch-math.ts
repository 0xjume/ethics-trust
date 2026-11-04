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

/** Curve fields the contract stores and quotes. Borsh of this is in `params_hash`. */
export type StoredCurve = {
  sqrtStartPrice: string;
  migrationQuoteThreshold: string;
  collectFeeMode: number;
  enableFirstSwapWithMinFee: boolean;
  baseFeeMode: number;
  cliffFeeNumerator: string;
  numberOfPeriod: number;
  reductionFactor: string;
  points: { sqrtPrice: string; liquidity: string }[];
};

function leBytes(value: bigint, width: number): Buffer {
  const out = Buffer.alloc(width);
  let n = value;
  for (let i = 0; i < width; i++) {
    out[i] = Number(n & 0xffn);
    n >>= 8n;
  }
  return out;
}

/** Borsh layout of `DbcCurve`: u128, u64, u8, bool, u8, u64, u16, u64, vec of (u128, u128). */
export function borshStoredCurve(curve: StoredCurve): Buffer {
  const parts = [
    leBytes(BigInt(curve.sqrtStartPrice), 16),
    leBytes(BigInt(curve.migrationQuoteThreshold), 8),
    Buffer.from([curve.collectFeeMode]),
    Buffer.from([curve.enableFirstSwapWithMinFee ? 1 : 0]),
    Buffer.from([curve.baseFeeMode]),
    leBytes(BigInt(curve.cliffFeeNumerator), 8),
    leBytes(BigInt(curve.numberOfPeriod), 2),
    leBytes(BigInt(curve.reductionFactor), 8),
    leBytes(BigInt(curve.points.length), 4),
  ];
  for (const point of curve.points) {
    parts.push(leBytes(BigInt(point.sqrtPrice), 16));
    parts.push(leBytes(BigInt(point.liquidity), 16));
  }
  return Buffer.concat(parts);
}

/**
 * Same pin as the contract: hex(sha256(config \\0 name \\0 symbol \\0 uri || borsh(curve))).
 * `periodFrequency` is not in the curve. A fresh pool is period 0, or the
 * linear minimum when first-swap min fee is on.
 */
export async function pinParamsHash(
  dbcConfig: string,
  name: string,
  symbol: string,
  uri: string,
  curve: StoredCurve,
): Promise<string> {
  const { createHash } = await import("node:crypto");
  const prefix = Buffer.from(`${dbcConfig}\0${name}\0${symbol}\0${uri}`, "utf8");
  return createHash("sha256")
    .update(Buffer.concat([prefix, borshStoredCurve(curve)]))
    .digest("hex");
}
