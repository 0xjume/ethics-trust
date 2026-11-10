import { existsSync, readFileSync } from "node:fs";
import { Keypair } from "@solana/web3.js";
import { KeyPair, type KeyPairString } from "near-api-js";

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

export function encodeBase58(bytes: Uint8Array): string {
  let zeroes = 0;
  while (zeroes < bytes.length && bytes[zeroes] === 0) zeroes += 1;
  const digits = [0];
  for (let i = zeroes; i < bytes.length; i += 1) {
    let carry = bytes[i]!;
    for (let j = 0; j < digits.length; j += 1) {
      carry += digits[j]! << 8;
      digits[j] = carry % 58;
      carry = Math.floor(carry / 58);
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = Math.floor(carry / 58);
    }
  }
  let out = "1".repeat(zeroes);
  for (let i = digits.length - 1; i >= 0; i -= 1) out += ALPHABET[digits[i]!]!;
  return out;
}

export function decodeBase58(text: string): Uint8Array {
  if (text.length === 0) throw new Error("empty base58");
  const bytes = [0];
  for (const char of text) {
    const value = ALPHABET.indexOf(char);
    if (value < 0) throw new Error("invalid base58");
    let carry = value;
    for (let j = 0; j < bytes.length; j += 1) {
      carry += bytes[j]! * 58;
      bytes[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  let zeroes = 0;
  while (zeroes < text.length && text[zeroes] === "1") zeroes += 1;
  const out = new Uint8Array(zeroes + bytes.length);
  for (let i = 0; i < bytes.length; i += 1) out[out.length - 1 - i] = bytes[i]!;
  return out;
}

function keypairFromBytes(raw: unknown): Keypair {
  if (!Array.isArray(raw) || raw.length === 0 || raw.some((n) => typeof n !== "number")) {
    throw new Error("relayer keypair JSON must be an array of secret-key bytes");
  }
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

/**
 * `SOLANA_RELAYER_KEYPAIR` is a JSON secret-key file (the existing scripts),
 * a JSON array, or a base58 secret key. `SOLANA_KEYPAIR_PATH` is the same file.
 */
export function loadSolanaRelayerKeypair(
  env: Record<string, string | undefined> = process.env,
): Keypair {
  const value = env.SOLANA_RELAYER_KEYPAIR ?? env.SOLANA_KEYPAIR_PATH;
  if (!value) {
    throw new Error(
      "Set SOLANA_RELAYER_KEYPAIR to a JSON secret-key file, a JSON byte array, or a base58 secret key.",
    );
  }
  const trimmed = value.trim();
  if (trimmed.startsWith("[")) return keypairFromBytes(JSON.parse(trimmed));
  if (trimmed.startsWith("/") || trimmed.endsWith(".json") || existsSync(trimmed)) {
    if (!existsSync(trimmed)) {
      throw new Error(`SOLANA_RELAYER_KEYPAIR file not found: ${trimmed}`);
    }
    return keypairFromBytes(JSON.parse(readFileSync(trimmed, "utf8")));
  }
  const bytes = decodeBase58(trimmed);
  if (bytes.length !== 64) {
    throw new Error("SOLANA_RELAYER_KEYPAIR base58 value must decode to 64 bytes");
  }
  return Keypair.fromSecretKey(bytes);
}

export function loadWatcherPrivateKey(
  env: Record<string, string | undefined> = process.env,
): string {
  const inline = env.NEAR_WATCHER_PRIVATE_KEY?.trim();
  const path = env.NEAR_WATCHER_KEY_PATH?.trim();
  let key = inline ?? "";
  if (!key && path) {
    if (!existsSync(path)) throw new Error(`NEAR_WATCHER_KEY_PATH file not found: ${path}`);
    const text = readFileSync(path, "utf8").trim();
    if (text.startsWith("{")) {
      const parsed = JSON.parse(text) as { private_key?: string; secret_key?: string };
      key = parsed.private_key ?? parsed.secret_key ?? "";
    } else {
      key = text;
    }
  }
  if (!key.startsWith("ed25519:")) {
    throw new Error(
      "Set NEAR_WATCHER_PRIVATE_KEY or NEAR_WATCHER_KEY_PATH to an ed25519: secret key.",
    );
  }
  KeyPair.fromString(key as KeyPairString);
  return key;
}
