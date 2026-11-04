/** Display helpers shared by the Trust room. Amounts stay strings so u64/u128 stay exact. */

const LAMPORTS = 1_000_000_000n;
const TOKEN_BASE = 1_000_000n;

export function asBigInt(value) {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("amount is not finite");
    return BigInt(Math.trunc(value));
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) return BigInt(value);
  throw new Error("amount is not an integer");
}

/** Quote JSON integers that do not fit in a JS number before JSON.parse. */
export function parseNearJson(text) {
  if (text === "" || text == null) return null;
  const quoted = String(text).replace(/(-?\d{16,})(?=(?:[^"\\]*"[^"\\]*")*[^"]*$)/g, '"$1"');
  return JSON.parse(quoted);
}

export function formatUnits(raw, decimals, maxFrac = decimals) {
  const n = asBigInt(raw);
  const negative = n < 0n;
  const v = negative ? -n : n;
  const base = 10n ** BigInt(decimals);
  const whole = v / base;
  let frac = (v % base).toString().padStart(decimals, "0");
  if (maxFrac < decimals) frac = frac.slice(0, maxFrac);
  frac = frac.replace(/0+$/, "");
  const body = frac ? `${whole}.${frac}` : `${whole}`;
  return negative ? `-${body}` : body;
}

export function formatSol(lamports) {
  return formatUnits(lamports, 9, 6);
}

export function formatTokens(raw) {
  return formatUnits(raw, 6, 6);
}

/** Basis points of `part / whole`, shown as a percent with 2 decimal places. */
export function sharePercent(part, whole) {
  const p = asBigInt(part);
  const w = asBigInt(whole);
  if (w <= 0n || p <= 0n) return "0.00";
  const bps = (p * 10_000n) / w;
  const wholePct = bps / 100n;
  const frac = (bps % 100n).toString().padStart(2, "0");
  return `${wholePct}.${frac}`;
}

/** Floor share. Dust stays in the raise ATA and is not a share. */
export function proRataShare(tokensBought, credited, creditedTotal) {
  const tokens = asBigInt(tokensBought);
  const part = asBigInt(credited);
  const total = asBigInt(creditedTotal);
  if (tokens <= 0n || part <= 0n || total <= 0n) return 0n;
  return (tokens * part) / total;
}

export function formatNearTime(ns) {
  if (ns == null || ns === "") return "—";
  const ms = asBigInt(ns) / 1_000_000n;
  const date = new Date(Number(ms));
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat("en-GB", {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date) + " UTC";
}

export function shortKey(value, head = 4, tail = 4) {
  const text = String(value ?? "");
  if (text.length <= head + tail + 1) return text;
  return `${text.slice(0, head)}…${text.slice(-tail)}`;
}

export const TRUST_STATES = [
  "Raising",
  "Filled",
  "Executing",
  "Launched",
  "Distributed",
  "Refunding",
];

export function statusLabel(state) {
  return TRUST_STATES.includes(state) ? state : "Unknown";
}

export function solanaExplorerAddress(address) {
  return `https://explorer.solana.com/address/${address}?cluster=devnet`;
}

export function solanaExplorerTx(signature) {
  return `https://explorer.solana.com/tx/${signature}?cluster=devnet`;
}

export function nearblocksAccount(accountId) {
  return `https://testnet.nearblocks.io/address/${accountId}`;
}

export function nearblocksTx(hash) {
  return `https://testnet.nearblocks.io/txns/${hash}`;
}

/**
 * IterableMap ledger index under prefix `cv`. Each value is a Borsh string
 * `trustId:solanaPubkey`.
 */
export function ledgerKeysFromState(values) {
  const rows = [];
  for (const entry of values ?? []) {
    const raw = entry?.value;
    if (typeof raw !== "string") continue;
    const bytes = decodeBase64(raw);
    if (bytes.length < 4) continue;
    const len = bytes[0] | (bytes[1] << 8) | (bytes[2] << 16) | (bytes[3] << 24);
    if (len <= 0 || bytes.length < 4 + len) continue;
    const text = new TextDecoder().decode(bytes.subarray(4, 4 + len));
    const split = text.indexOf(":");
    if (split <= 0) continue;
    const trustId = text.slice(0, split);
    const contributor = text.slice(split + 1);
    if (!/^\d+$/.test(trustId) || contributor.length < 32) continue;
    rows.push({ trustId, contributor });
  }
  return rows;
}

export function countContributors(rows, trustId) {
  const id = String(trustId);
  return rows.filter((row) => row.trustId === id).length;
}

function decodeBase64(value) {
  if (typeof Buffer !== "undefined") return Uint8Array.from(Buffer.from(value, "base64"));
  const bin = atob(value);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

export function lamportsToSolInput(lamports) {
  return formatUnits(lamports, 9, 9);
}

export function solToLamports(sol) {
  const text = String(sol).trim();
  if (!/^\d+(\.\d+)?$/.test(text)) throw new Error("SOL amount is invalid");
  const [whole, frac = ""] = text.split(".");
  if (frac.length > 9) throw new Error("SOL amount has more than 9 decimals");
  const padded = frac.padEnd(9, "0");
  return BigInt(whole) * LAMPORTS + BigInt(padded);
}

export { LAMPORTS, TOKEN_BASE };
