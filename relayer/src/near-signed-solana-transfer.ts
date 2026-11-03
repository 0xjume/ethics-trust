/**
 * NEAR-signed Solana devnet transfer — DO NOT RUN in CI or this environment.
 *
 * Needs a funded NEAR testnet account (1 yocto + gas) and a funded Solana
 * devnet keypair only as an optional co-signer / destination. The derived
 * vault must already hold SOL. This script is the day-1 spike (d) recipe.
 *
 * Env:
 *   NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY
 *   NEAR_RPC_URL (default testnet)
 *   MPC_CONTRACT (default v1.signer-prod.testnet)
 *   NEAR_PREDECESSOR (account that owns the path; usually the Trust contract)
 *   TRUST_PATH (default trust/0/raise)
 *   SOLANA_RPC_URL (default devnet)
 *   SOLANA_TRANSFER_TO (base58)
 *   SOLANA_TRANSFER_LAMPORTS (default 5000)
 *
 * Measures: NEAR gas attached, wall-clock latency of sign + submit.
 */
import { Account, JsonRpcProvider, KeyPairString, teraToGas } from "near-api-js";
import {
  Connection,
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

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Set ${name} in the environment. See .env.example.`);
  }
  return value;
}

export async function runNearSignedSolanaTransfer(): Promise<void> {
  const started = Date.now();
  const accountId = requireEnv("NEAR_ACCOUNT_ID");
  const privateKey = requireEnv("NEAR_PRIVATE_KEY") as KeyPairString;
  const destination = new PublicKey(requireEnv("SOLANA_TRANSFER_TO"));
  const predecessor = process.env.NEAR_PREDECESSOR ?? accountId;
  const path = process.env.TRUST_PATH ?? "trust/0/raise";
  const lamports = Number(process.env.SOLANA_TRANSFER_LAMPORTS ?? "5000");
  const solanaRpc = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

  const nearKey = await viewDerivedPublicKey({
    path,
    predecessor,
    domain_id: 1,
  });
  const vault = new PublicKey(solanaAddressFromNearKey(nearKey));
  console.log(JSON.stringify({ derivedVault: vault.toBase58(), path, predecessor }));

  const connection = new Connection(solanaRpc, "confirmed");
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash(
    "confirmed",
  );
  const tx = new Transaction();
  tx.feePayer = vault;
  tx.recentBlockhash = blockhash;
  tx.add(
    SystemProgram.transfer({
      fromPubkey: vault,
      toPubkey: destination,
      lamports,
    }),
  );
  const message = tx.serializeMessage();
  const payloadHex = Buffer.from(message).toString("hex");

  const provider = new JsonRpcProvider({
    url: process.env.NEAR_RPC_URL ?? TESTNET_RPC,
  });
  const account = new Account(accountId, provider, privateKey);

  const signStarted = Date.now();
  const attachedTgas = 250;
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
  if (!tx.verifySignatures()) {
    throw new Error("assembled Solana signatures failed local verify");
  }

  const submitStarted = Date.now();
  const signature = await connection.sendRawTransaction(tx.serialize(), {
    skipPreflight: false,
  });
  await connection.confirmTransaction(
    { signature, blockhash, lastValidBlockHeight },
    "confirmed",
  );
  const submitMs = Date.now() - submitStarted;

  console.log(
    JSON.stringify(
      {
        solanaSignature: signature,
        attachedTgas,
        signMs,
        submitMs,
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
            "Refusing to run: needs a funded NEAR testnet account and a funded derived Solana vault. Set EXECUTE_NEAR_SIGNED_TRANSFER=1 after exporting the env vars in .env.example.",
        },
        null,
        2,
      ),
    );
    process.exit(0);
  }
  await runNearSignedSolanaTransfer();
}
