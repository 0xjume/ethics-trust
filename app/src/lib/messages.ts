import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferCheckedInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { PublicKey, SystemProgram, Transaction } from "@solana/web3.js";

export const TOKEN_DECIMALS = 6;
export const MAX_DISTRIBUTE_BATCH = 5;

/**
 * Vault-source System transfer. The connected wallet is the fee payer
 * (account 0) and still has to co-sign on Solana. The NEAR contract signs
 * the deposit or raise path. Shape A / raise-refund accept this layout.
 */
export function buildRefundMessage(args: {
  feePayer: PublicKey;
  source: PublicKey;
  destination: PublicKey;
  lamports: bigint;
  blockhash: string;
}): Uint8Array {
  if (args.lamports <= 0n) throw new Error("Refund amount must be greater than 0");
  if (args.lamports > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("Refund amount does not fit a client-built transfer");
  }
  const tx = new Transaction({
    feePayer: args.feePayer,
    recentBlockhash: args.blockhash,
  });
  tx.add(
    SystemProgram.transfer({
      fromPubkey: args.source,
      toPubkey: args.destination,
      lamports: Number(args.lamports),
    }),
  );
  return assertLegacy(tx.serializeMessage());
}

/**
 * Shape D: relayer-paid CreateIdempotent + TransferChecked.
 * `feePayer` must not be the raise, mint, or fee vault. Amounts are raw
 * 6-decimal units and must equal each contributor's remaining claimable.
 */
export function buildDistributeMessage(args: {
  feePayer: PublicKey;
  raise: PublicKey;
  mint: PublicKey;
  blockhash: string;
  legs: Array<{ owner: PublicKey; amount: bigint; createAta?: boolean }>;
}): Uint8Array {
  if (args.legs.length < 1 || args.legs.length > MAX_DISTRIBUTE_BATCH) {
    throw new Error(`A distribute batch is 1 to ${MAX_DISTRIBUTE_BATCH} contributors`);
  }
  const tx = new Transaction({
    feePayer: args.feePayer,
    recentBlockhash: args.blockhash,
  });
  const source = getAssociatedTokenAddressSync(
    args.mint,
    args.raise,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  const seen = new Set<string>();
  for (const leg of args.legs) {
    const owner = leg.owner.toBase58();
    if (seen.has(owner)) throw new Error("Duplicate contributor in the batch");
    seen.add(owner);
    if (leg.amount <= 0n || leg.amount > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error("Claim amount must be a positive raw token amount");
    }
    const dest = getAssociatedTokenAddressSync(
      args.mint,
      leg.owner,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    if (leg.createAta !== false) {
      tx.add(
        createAssociatedTokenAccountIdempotentInstruction(
          args.feePayer,
          dest,
          leg.owner,
          args.mint,
          TOKEN_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      );
    }
    tx.add(
      createTransferCheckedInstruction(
        source,
        args.mint,
        dest,
        args.raise,
        Number(leg.amount),
        TOKEN_DECIMALS,
        [],
        TOKEN_PROGRAM_ID,
      ),
    );
  }
  const message = assertLegacy(tx.serializeMessage());
  const headerSigs = message[0];
  if (headerSigs !== 2) {
    throw new Error("Distribute message must be signed by the fee payer and the raise vault");
  }
  return message;
}

export function bytesToBase64(bytes: Uint8Array): string {
  if (typeof Buffer !== "undefined") return Buffer.from(bytes).toString("base64");
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function assertLegacy(message: Buffer): Uint8Array {
  if (message.length > 1232) throw new Error("Solana message is over the 1232-byte Chain Signatures limit");
  if ((message[0] ?? 0) & 0x80) throw new Error("Versioned messages are rejected by the contract");
  return Uint8Array.from(message);
}
