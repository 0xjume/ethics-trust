/**
 * Read contributor keys and pending rows out of contract state.
 * The web app uses the same `cv` index. Pending is a LookupMap whose
 * storage key is `p` plus the borsh string of the message hash, and whose
 * value is the borsh `PendingTransfer`. There is no list view for either.
 */

export type LedgerKey = { trustId: string; contributor: string };

export type PendingRow = {
  trustId: string;
  contributor: string;
  kind: string;
  lamports: bigint;
  messageHash: string;
  expiresAtNs: bigint;
  settled: boolean;
  released: boolean;
  superseded: boolean;
};

export function ledgerKeysFromState(
  values: Array<{ value?: string }>,
): LedgerKey[] {
  const rows: LedgerKey[] = [];
  for (const entry of values) {
    if (typeof entry?.value !== "string") continue;
    const bytes = Buffer.from(entry.value, "base64");
    if (bytes.length < 4) continue;
    const len = bytes.readUInt32LE(0);
    if (len <= 0 || bytes.length < 4 + len) continue;
    const text = bytes.subarray(4, 4 + len).toString("utf8");
    const split = text.indexOf(":");
    if (split <= 0) continue;
    const trustId = text.slice(0, split);
    const contributor = text.slice(split + 1);
    if (!/^\d+$/.test(trustId) || contributor.length < 32) continue;
    rows.push({ trustId, contributor });
  }
  return rows;
}

export function decodePending(bytes: Uint8Array): PendingRow | null {
  const buf = Buffer.from(bytes);
  try {
    let i = 0;
    const u32 = () => {
      if (i + 4 > buf.length) throw new Error("truncated");
      const n = buf.readUInt32LE(i);
      i += 4;
      return n;
    };
    const u64 = () => {
      if (i + 8 > buf.length) throw new Error("truncated");
      let n = 0n;
      for (let b = 0; b < 8; b += 1) n |= BigInt(buf[i + b]!) << BigInt(8 * b);
      i += 8;
      return n;
    };
    const str = () => {
      const n = u32();
      if (i + n > buf.length) throw new Error("truncated");
      const text = buf.subarray(i, i + n).toString("utf8");
      i += n;
      return text;
    };
    const flag = () => {
      if (i >= buf.length) throw new Error("truncated");
      const bit = buf[i] !== 0;
      i += 1;
      return bit;
    };
    const trustId = u64().toString();
    const contributor = str();
    const kind = str();
    const lamports = u64();
    const messageHash = str();
    const expiresAtNs = u64();
    const settled = flag();
    const released = flag();
    u64();
    const superseded = flag();
    str();
    if (!/^[0-9a-f]{64}$/.test(messageHash)) return null;
    if (i !== buf.length) return null;
    return {
      trustId,
      contributor,
      kind,
      lamports,
      messageHash,
      expiresAtNs,
      settled,
      released,
      superseded,
    };
  } catch {
    return null;
  }
}
