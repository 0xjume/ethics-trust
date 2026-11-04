/**
 * Create a real Solana **devnet** DBC config with createConfig2.
 *
 * feeClaimer = trust fee vault (`trust/<id>/fees`)
 * leftoverReceiver = trust raise vault (`trust/<id>/raise`)
 * quote = native SOL
 *
 * Dry-run unless EXECUTE_CREATE_CONFIG=1. Relayer pays and signs.
 *
 * Env: CONTRACT_ID or NEAR_PREDECESSOR, TRUST_ID, SOLANA_RELAYER_KEYPAIR,
 *      SOLANA_RPC_URL, SOLANA_SUBMIT (implied by EXECUTE_CREATE_CONFIG).
 */
import { readFileSync } from "node:fs";
import { NATIVE_MINT } from "@solana/spl-token";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { DynamicBondingCurveClient } from "@meteora-ag/dynamic-bonding-curve-sdk";
import { demoCurveConfig } from "./dbc-curve.ts";
import { storedCurveFromConfig } from "./dbc-quote.ts";
import { deriveTrustAddresses } from "./derive-addresses.ts";
import { pinParamsHash } from "./launch-math.ts";

const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

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

export function leftoverReceiverIsRaise(): true {
  return true;
}

export async function buildCreateConfig2Plan(options?: {
  predecessor?: string;
  trustId?: string;
}) {
  const predecessor =
    options?.predecessor ??
    process.env.CONTRACT_ID ??
    process.env.NEAR_PREDECESSOR ??
    "example.testnet";
  const trustId = options?.trustId ?? process.env.TRUST_ID ?? "1";
  const rows = await deriveTrustAddresses({
    predecessor,
    paths: [`trust/${trustId}/raise`, `trust/${trustId}/fees`, `trust/${trustId}/mint`],
  });
  const raise = rows.find((r) => r.path.endsWith("/raise"))?.solana_address;
  const fees = rows.find((r) => r.path.endsWith("/fees"))?.solana_address;
  const mint = rows.find((r) => r.path.endsWith("/mint"))?.solana_address;
  if (!raise || !fees || !mint) {
    throw new Error("failed to derive raise/fees/mint");
  }
  return {
    predecessor,
    trustId,
    raise,
    fees,
    mint,
    leftoverReceiver: raise,
    feeClaimer: fees,
    quoteMint: NATIVE_MINT.toBase58(),
    curve: "demoCurveConfig (Ethics-scaled, SOL-quoted)",
  };
}

export async function runCreateDbcConfig(): Promise<void> {
  const plan = await buildCreateConfig2Plan();
  const config = Keypair.generate();
  const name = process.env.TOKEN_NAME ?? "Keyless";
  const symbol = process.env.TOKEN_SYMBOL ?? "KLT";
  const uri = process.env.TOKEN_URI ?? "https://ethics.ltd/k.json";
  const paramsHash = await pinParamsHash(
    config.publicKey.toBase58(),
    name,
    symbol,
    uri,
    storedCurveFromConfig(demoCurveConfig()),
  );

  const report: Record<string, unknown> = {
    ...plan,
    config: config.publicKey.toBase58(),
    name,
    symbol,
    uri,
    paramsHash,
    leftoverReceiver: plan.raise,
    feeClaimer: plan.fees,
  };

  if (process.env.EXECUTE_CREATE_CONFIG !== "1") {
    console.log(
      JSON.stringify(
        {
          ...report,
          submitted: false,
          reason:
            "Dry-run. Set EXECUTE_CREATE_CONFIG=1 and SOLANA_RELAYER_KEYPAIR to send createConfig2 on devnet.",
        },
        null,
        2,
      ),
    );
    return;
  }

  const relayer = loadRelayerKeypair();
  const connection = new Connection(RPC, "confirmed");
  const client = new DynamicBondingCurveClient(connection, "confirmed");
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash(
    "confirmed",
  );
  const tx = await client.partner.createConfig2({
    payer: relayer.publicKey,
    config: config.publicKey,
    feeClaimer: new PublicKey(plan.fees),
    leftoverReceiver: new PublicKey(plan.raise),
    quoteMint: NATIVE_MINT,
    transferFeeParameters: null,
    ...demoCurveConfig(),
  });
  tx.feePayer = relayer.publicKey;
  tx.recentBlockhash = blockhash;
  tx.partialSign(relayer, config);
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
        next: `create_trust with dbc_config=${config.publicKey.toBase58()} name=${name} symbol=${symbol} uri=${uri}`,
      },
      null,
      2,
    ),
  );
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  await runCreateDbcConfig();
}
