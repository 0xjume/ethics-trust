/**
 * Assemble the Shape D Solana tx: relayer fee-payer signature + the one
 * raise-vault MPC signature returned by `request_distribute`.
 * Submit only when SOLANA_SUBMIT=1.
 *
 * Env: DISTRIBUTE_MESSAGE_B64, RAISE_SIGNATURE (64-byte hex or JSON),
 *      RAISE_ADDRESS, SOLANA_RELAYER_KEYPAIR, SOLANA_RPC_URL.
 */
import { readFileSync } from "node:fs";
import { Connection, Keypair, Message, PublicKey, Transaction } from "@solana/web3.js";
import { extractEd25519Signature } from "./relay-launch.ts";

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

function parseSig(raw: string): Uint8Array {
  const trimmed = raw.trim();
  if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
    return extractEd25519Signature(JSON.parse(trimmed));
  }
  return extractEd25519Signature(trimmed);
}

export async function runRelayDistribute(): Promise<void> {
  const messageB64 = process.env.DISTRIBUTE_MESSAGE_B64;
  const raiseSigRaw = process.env.RAISE_SIGNATURE;
  const raiseAddr = process.env.RAISE_ADDRESS;
  if (!messageB64 || !raiseSigRaw || !raiseAddr) {
    throw new Error("Set DISTRIBUTE_MESSAGE_B64, RAISE_SIGNATURE, RAISE_ADDRESS.");
  }
  const message = Buffer.from(messageB64, "base64");
  const tx = Transaction.populate(Message.from(message));
  const relayer = loadRelayerKeypair();
  tx.addSignature(new PublicKey(raiseAddr), Buffer.from(parseSig(raiseSigRaw)));
  tx.partialSign(relayer);
  if (!tx.verifySignatures()) {
    throw new Error("assembled Solana signatures failed local verify");
  }

  const report: Record<string, unknown> = {
    messageBytes: message.length,
    relayerFeePayer: relayer.publicKey.toBase58(),
    raise: raiseAddr,
    vaultSignaturesVerified: true,
  };

  if (process.env.SOLANA_SUBMIT !== "1") {
    console.log(
      JSON.stringify(
        {
          ...report,
          submitted: false,
          reason: "Set SOLANA_SUBMIT=1 to send the assembled distribute tx.",
        },
        null,
        2,
      ),
    );
    return;
  }

  const connection = new Connection(
    process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com",
    "confirmed",
  );
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash(
    "confirmed",
  );
  const signature = await connection.sendRawTransaction(tx.serialize(), {
    skipPreflight: false,
  });
  await connection.confirmTransaction(
    { signature, blockhash, lastValidBlockHeight },
    "confirmed",
  );
  console.log(
    JSON.stringify(
      {
        ...report,
        submitted: true,
        solanaSignature: signature,
        explorer: `https://solscan.io/tx/${signature}?cluster=devnet`,
      },
      null,
      2,
    ),
  );
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  await runRelayDistribute();
}
