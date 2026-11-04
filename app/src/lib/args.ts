/** Wallet JSON cannot carry bigint. u64 fields become numbers; U128 strings stay strings. */
export function toWalletArgs(value: unknown): unknown {
  if (typeof value === "bigint") return Number(value);
  if (Array.isArray(value)) return value.map(toWalletArgs);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value)) out[key] = toWalletArgs(item);
    return out;
  }
  return value;
}
