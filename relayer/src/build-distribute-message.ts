/**
 * Build a legacy Shape D message: optional CreateIdempotent ATA (payer =
 * relayer) + SPL TransferChecked from the raise vault's token ATA to the
 * contributor ATA recorded at register. Classic Tokenkeg, 6 decimals.
 * One raise MPC sign. No other instructions. Does not send.
 *
 * Env: CONTRACT_ID, TRUST_ID, SOLANA_RELAYER_KEYPAIR, SOLANA_RPC_URL.
 * Claims: DISTRIBUTE_CLAIMS='[{"owner":"<b58>","amount":"<raw>","createAta":true}]'
 * or one leg via CONTRIBUTOR_OWNER plus AMOUNT, or
 * TOKENS_BOUGHT + CREDITED_TOTAL + CONTRIBUTOR_CREDITED (floor share).
 */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferCheckedInstruction,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Connection, Keypair, PublicKey, Transaction } from "@solana/web3.js";
import { PACKET_LIMIT } from "./dbc-tx-size.ts";
import { deriveTrustAddresses } from "./derive-addresses.ts";
import {
  DISTRIBUTE_ATTACH_TGAS,
  MAX_DISTRIBUTE_BATCH,
  proRataShare,
  TOKEN_DECIMALS,
} from "./distribute-math.ts";

const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

export type DistributeLegInput = {
  owner: PublicKey;
  amount: bigint;
  createAta: boolean;
};

function loadRelayerKeypair(): Keypair {
  const path =
    process.env.SOLANA_RELAYER_KEYPAIR ?? process.env.SOLANA_KEYPAIR_PATH;
  if (!path) {
    throw new Error("Set SOLANA_RELAYER_KEYPAIR (JSON secret-key file).");
  }
  const raw = JSON.parse(readFileSync(path, "utf8")) as unknown;
  if (!Array.isArray(raw) || raw.some((n) => typeof n !== "number")) {
    throw new Error("relayer keypair file must be a JSON array of secret-key bytes");
  }
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

/** Legacy message the contract will accept. Deterministic given the blockhash. */
export function buildDistributeMessageBytes(args: {
  relayer: PublicKey;
  raise: PublicKey;
  mint: PublicKey;
  legs: DistributeLegInput[];
  blockhash: string;
}): Buffer {
  if (args.legs.length < 1 || args.legs.length > MAX_DISTRIBUTE_BATCH) {
    throw new Error(`distribute batch must be 1..=${MAX_DISTRIBUTE_BATCH}`);
  }
  const seen = new Set<string>();
  const tx = new Transaction();
  const source = getAssociatedTokenAddressSync(
    args.mint,
    args.raise,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  for (const leg of args.legs) {
    const owner = leg.owner.toBase58();
    if (seen.has(owner)) {
      throw new Error(`duplicate contributor ${owner}`);
    }
    seen.add(owner);
    if (leg.amount <= 0n) {
      throw new Error("transfer amount must be > 0");
    }
    const dest = getAssociatedTokenAddressSync(
      args.mint,
      leg.owner,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    if (leg.createAta) {
      tx.add(
        createAssociatedTokenAccountIdempotentInstruction(
          args.relayer,
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
        leg.amount,
        TOKEN_DECIMALS,
        [],
        TOKEN_PROGRAM_ID,
      ),
    );
  }
  tx.feePayer = args.relayer;
  tx.recentBlockhash = args.blockhash;
  const compiled = tx.compileMessage();
  if (compiled.header.numRequiredSignatures !== 2) {
    throw new Error(
      `expected relayer + raise signers, got ${compiled.header.numRequiredSignatures}`,
    );
  }
  const message = tx.serializeMessage();
  if (message.length > PACKET_LIMIT) {
    throw new Error(`distribute message ${message.length} B exceeds ${PACKET_LIMIT}`);
  }
  if (message[0] & 0x80) {
    throw new Error("versioned / ALT message is not allowed");
  }
  return Buffer.from(message);
}

type ClaimJson = { owner?: string; amount?: string; createAta?: boolean };

function parseClaims(): DistributeLegInput[] {
  const raw = process.env.DISTRIBUTE_CLAIMS;
  if (raw) {
    const parsed = JSON.parse(raw) as ClaimJson[];
    if (!Array.isArray(parsed)) {
      throw new Error("DISTRIBUTE_CLAIMS must be a JSON array");
    }
    return parsed.map((row) => ({
      owner: new PublicKey(row.owner ?? ""),
      amount: BigInt(row.amount ?? "0"),
      createAta: row.createAta !== false,
    }));
  }
  const owner = process.env.CONTRIBUTOR_OWNER;
  if (!owner) {
    throw new Error("Set DISTRIBUTE_CLAIMS or CONTRIBUTOR_OWNER.");
  }
  let amount = process.env.AMOUNT;
  if (!amount) {
    const tokens = process.env.TOKENS_BOUGHT;
    const total = process.env.CREDITED_TOTAL;
    const credited = process.env.CONTRIBUTOR_CREDITED;
    if (!tokens || !total || !credited) {
      throw new Error(
        "Set AMOUNT, or TOKENS_BOUGHT + CREDITED_TOTAL + CONTRIBUTOR_CREDITED.",
      );
    }
    amount = proRataShare(BigInt(tokens), BigInt(credited), BigInt(total)).toString();
  }
  return [
    {
      owner: new PublicKey(owner),
      amount: BigInt(amount),
      createAta: process.env.CREATE_ATA !== "0",
    },
  ];
}

export async function buildDistributeMessage(): Promise<Record<string, unknown>> {
  const predecessor =
    process.env.CONTRACT_ID ?? process.env.NEAR_PREDECESSOR ?? "example.testnet";
  const trustId = process.env.TRUST_ID ?? "1";
  const legs = parseClaims();
  const rows = await deriveTrustAddresses({
    predecessor,
    paths: [`trust/${trustId}/raise`, `trust/${trustId}/mint`],
  });
  const raise = new PublicKey(
    rows.find((r) => r.path.endsWith("/raise"))!.solana_address,
  );
  const mint = new PublicKey(
    rows.find((r) => r.path.endsWith("/mint"))!.solana_address,
  );
  const relayer = loadRelayerKeypair();
  const connection = new Connection(RPC, "confirmed");
  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  const message = buildDistributeMessageBytes({
    relayer: relayer.publicKey,
    raise,
    mint,
    legs,
    blockhash,
  });
  return {
    trustId,
    predecessor,
    raise: raise.toBase58(),
    mint: mint.toBase58(),
    tokenProgram: TOKEN_PROGRAM_ID.toBase58(),
    decimals: TOKEN_DECIMALS,
    attachTgas: DISTRIBUTE_ATTACH_TGAS,
    maxBatch: MAX_DISTRIBUTE_BATCH,
    legs: legs.map((leg) => ({
      owner: leg.owner.toBase58(),
      amount: leg.amount.toString(),
      createAta: leg.createAta,
      destinationAta: getAssociatedTokenAddressSync(
        mint,
        leg.owner,
        false,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID,
      ).toBase58(),
    })),
    sourceAta: getAssociatedTokenAddressSync(
      mint,
      raise,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    ).toBase58(),
    messageBytes: message.length,
    packetLimit: PACKET_LIMIT,
    messageBase64: message.toString("base64"),
    messageHash: createHash("sha256").update(message).digest("hex"),
    relayerFeePayer: relayer.publicKey.toBase58(),
    recentBlockhash: blockhash,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const report = await buildDistributeMessage();
  console.log(JSON.stringify(report, null, 2));
}
