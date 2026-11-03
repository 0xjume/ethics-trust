/**
 * NEAR-signed Solana transfer — DO NOT RUN the MPC call in CI.
 *
 * The derived vault is a required signer of the System transfer. A separate
 * relayer keypair is the Solana fee payer and signs locally, so the vault
 * balance changes by exactly `lamports` (no fee deducted from the vault).
 *
 * Env:
 *   NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY
 *   NEAR_RPC_URL (default testnet)
 *   MPC_CONTRACT (default v1.signer-prod.testnet)
 *   NEAR_PREDECESSOR (account that owns the path)
 *   TRUST_PATH (default trust/1/raise)
 *   MPC_SIGN_TGAS (default DEFAULT_MPC_SIGN_TGAS)
 *   SOLANA_RPC_URL (default devnet)
 *   SOLANA_RELAYER_KEYPAIR / SOLANA_KEYPAIR_PATH — JSON secret-key array
 *   SOLANA_TRANSFER_TO (base58)
 *   SOLANA_TRANSFER_LAMPORTS (default 5000)
 *   SOLANA_SUBMIT=1 to send after local verify (needs relayer SOL on devnet)
 *
 * Live sign measurements (no Solana submit): docs/spikes.md.
 */
import { readFileSync } from "node:fs";
import { Account, JsonRpcProvider, KeyPairString, teraToGas } from "near-api-js";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import {
  MPC_CONTRACT,
  TESTNET_RPC,
  solanaAddressFromNearKey,
  viewDerivedPublicKey,
} from "./near-rpc.ts";

/** Attached gas for a direct `sign` on the MPC contract. Live burn was 8.84 TGas. */
export const DEFAULT_MPC_SIGN_TGAS = 30;

export function resolveAttachedTgas(
  env: Record<string, string | undefined> = process.env,
): number {
  const raw = env.MPC_SIGN_TGAS;
  if (raw === undefined || raw === "") {
    return DEFAULT_MPC_SIGN_TGAS;
  }
  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error("MPC_SIGN_TGAS must be a positive number");
  }
  return value;
}

export function buildRelayerPaidVaultTransfer(params: {
  relayer: PublicKey;
  vault: PublicKey;
  destination: PublicKey;
  lamports: number;
  blockhash: string;
}): Transaction {
  if (params.relayer.equals(params.vault)) {
    throw new Error("relayer fee payer must be a different key from the vault");
  }
  const tx = new Transaction();
  tx.feePayer = params.relayer;
  tx.recentBlockhash = params.blockhash;
  tx.add(
    SystemProgram.transfer({
      fromPubkey: params.vault,
      toPubkey: params.destination,
      lamports: params.lamports,
    }),
  );
  return tx;
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Set ${name} in the environment. See .env.example.`);
  }
  return value;
}

function loadRelayerKeypair(): Keypair {
  const path =
    process.env.SOLANA_RELAYER_KEYPAIR ?? process.env.SOLANA_KEYPAIR_PATH;
  if (!path) {
    throw new Error(
      "Set SOLANA_RELAYER_KEYPAIR (JSON secret-key file) for the local fee payer.",
    );
  }
  const raw = JSON.parse(readFileSync(path, "utf8")) as unknown;
  if (!Array.isArray(raw) || raw.some((n) => typeof n !== "number")) {
    throw new Error("relayer keypair file must be a JSON array of secret-key bytes");
  }
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

export async function runNearSignedSolanaTransfer(): Promise<void> {
  const started = Date.now();
  const accountId = requireEnv("NEAR_ACCOUNT_ID");
  const privateKey = requireEnv("NEAR_PRIVATE_KEY") as KeyPairString;
  const destination = new PublicKey(requireEnv("SOLANA_TRANSFER_TO"));
  const predecessor = process.env.NEAR_PREDECESSOR ?? accountId;
  const path = process.env.TRUST_PATH ?? "trust/1/raise";
  const lamports = Number(process.env.SOLANA_TRANSFER_LAMPORTS ?? "5000");
  const solanaRpc = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
  const attachedTgas = resolveAttachedTgas();
  const relayer = loadRelayerKeypair();

  const nearKey = await viewDerivedPublicKey({
    path,
    predecessor,
    domain_id: 1,
  });
  const vault = new PublicKey(solanaAddressFromNearKey(nearKey));
  if (relayer.publicKey.equals(vault)) {
    throw new Error("relayer keypair must not be the derived vault");
  }
  console.log(
    JSON.stringify({
      derivedVault: vault.toBase58(),
      relayerFeePayer: relayer.publicKey.toBase58(),
      path,
      predecessor,
      attachedTgas,
    }),
  );

  const connection = new Connection(solanaRpc, "confirmed");
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash(
    "confirmed",
  );
  const tx = buildRelayerPaidVaultTransfer({
    relayer: relayer.publicKey,
    vault,
    destination,
    lamports,
    blockhash,
  });
  const message = tx.serializeMessage();
  const payloadHex = Buffer.from(message).toString("hex");

  const provider = new JsonRpcProvider({
    url: process.env.NEAR_RPC_URL ?? TESTNET_RPC,
  });
  const account = new Account(accountId, provider, privateKey);

  const signStarted = Date.now();
  const result = await account.callFunction({
    contractId: MPC_CONTRACT,
    methodName: "sign",
    args: {
      request: {
        path,
        payload_v2: { Eddsa: payloadHex },
        domain_id: 1,
      },
    },
    gas: teraToGas(attachedTgas),
    deposit: 1n,
  });
  const signMs = Date.now() - signStarted;

  const signatureBytes = extractEd25519Signature(result);
  tx.addSignature(vault, Buffer.from(signatureBytes));
  tx.partialSign(relayer);
  if (!tx.verifySignatures()) {
    throw new Error("assembled Solana signatures failed local verify");
  }

  const report: Record<string, unknown> = {
    derivedVault: vault.toBase58(),
    relayerFeePayer: relayer.publicKey.toBase58(),
    attachedTgas,
    messageBytes: message.length,
    signMs,
    vaultSignatureVerified: true,
  };

  if (process.env.SOLANA_SUBMIT !== "1") {
    console.log(
      JSON.stringify(
        {
          ...report,
          submitted: false,
          reason:
            "Solana submit skipped. Set SOLANA_SUBMIT=1 to send the assembled transaction.",
          totalMs: Date.now() - started,
        },
        null,
        2,
      ),
    );
    return;
  }

  const submitStarted = Date.now();
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
        submitMs: Date.now() - submitStarted,
        lastValidBlockHeight,
        totalMs: Date.now() - started,
        explorer: `https://solscan.io/tx/${signature}?cluster=devnet`,
      },
      null,
      2,
    ),
  );
}

function extractEd25519Signature(result: unknown): Uint8Array {
  const asObj = result as {
    Ed25519?: { signature?: number[] };
    signature?: number[];
  };
  const bytes = asObj?.Ed25519?.signature ?? asObj?.signature;
  if (!bytes || bytes.length !== 64) {
    throw new Error(`unexpected MPC sign response: ${JSON.stringify(result)}`);
  }
  return Uint8Array.from(bytes);
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  if (process.env.EXECUTE_NEAR_SIGNED_TRANSFER !== "1") {
    console.log(
      JSON.stringify(
        {
          skipped: true,
          reason:
            "Refusing to run: set EXECUTE_NEAR_SIGNED_TRANSFER=1 after exporting the env vars in .env.example. Solana submit additionally needs SOLANA_SUBMIT=1.",
        },
        null,
        2,
      ),
    );
    process.exit(0);
  }
  await runNearSignedSolanaTransfer();
}
