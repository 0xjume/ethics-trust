/** Integers that do not fit in a JS number stay strings. Same rule as `packages/ui`. */

export function asBigInt(value: unknown): bigint {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("amount is not finite");
    return BigInt(Math.trunc(value));
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) return BigInt(value);
  throw new Error("amount is not an integer");
}

/** Quote JSON integers that do not fit in a JS number before JSON.parse. */
export function parseNearJson(text: string): unknown {
  if (text === "" || text == null) return null;
  const quoted = String(text).replace(
    /(-?\d{16,})(?=(?:[^"\\]*"[^"\\]*")*[^"]*$)/g,
    '"$1"',
  );
  return JSON.parse(quoted);
}
