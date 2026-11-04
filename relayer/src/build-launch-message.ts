/**
 * Build a legacy Shape C message: optional fill-fee System transfer +
 * DBC createPoolWithFirstBuy (initialize_virtual_pool_with_spl_token + swap).
 *
 * poolCreator = buyer = raise vault
 * init / ATA payer = relayer (funds pool + ATA rent; raise does not top up)
 * baseMint = derived trust/<id>/mint (no random keypair)
 * first buy = credited_total − 1.5% fill fee
 * min out = 1
 *
 * Relayer is the Solana fee payer and the init/ATA rent payer. Does not send.
 *
 * Env: CONTRACT_ID, TRUST_ID, DBC_CONFIG, CREDITED_LAMPORTS,
 *      TOKEN_NAME / TOKEN_SYMBOL / TOKEN_URI, SOLANA_RELAYER_KEYPAIR,
 *      SOLANA_RPC_URL, FILL_FEE_ALREADY_PAID=1 to omit the fee transfer.
 */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { ASSOCIATED_TOKEN_PROGRAM_ID, NATIVE_MINT } from "@solana/spl-token";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import { DynamicBondingCurveClient } from "@meteora-ag/dynamic-bonding-curve-sdk";
import BN from "bn.js";
import { demoCurveConfig } from "./dbc-curve.ts";
import { deriveTrustAddresses } from "./derive-addresses.ts";
import { firstBuyLamports, fillFeeLamports, pinParamsHash } from "./launch-math.ts";
import { PACKET_LIMIT } from "./dbc-tx-size.ts";

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

export function prependFillFee(
  tx: Transaction,
  raise: PublicKey,
  feeVault: PublicKey,
  lamports: number,
): Transaction {
  if (lamports <= 0) {
    return tx;
  }
  const out = new Transaction();
  out.add(
    SystemProgram.transfer({
      fromPubkey: raise,
      toPubkey: feeVault,
      lamports,
    }),
  );
  for (const ix of tx.instructions) {
    out.add(ix);
  }
  out.feePayer = tx.feePayer;
  out.recentBlockhash = tx.recentBlockhash;
  return out;
}

/** SDK first-buy ATA creates use buyer=raise as payer. Point them at the relayer. */
export function retargetAtaPayer(
  tx: Transaction,
  from: PublicKey,
  to: PublicKey,
): Transaction {
  for (const ix of tx.instructions) {
    if (!ix.programId.equals(ASSOCIATED_TOKEN_PROGRAM_ID)) {
      continue;
    }
    if (ix.keys[0] && ix.keys[0].pubkey.equals(from)) {
      ix.keys[0].pubkey = to;
      ix.keys[0].isSigner = true;
    }
  }
  return tx;
}

export function finalizeLaunchMessage(
  poolTx: Transaction,
  raise: PublicKey,
  feeVault: PublicKey,
  relayer: PublicKey,
  fillFee: number,
  blockhash: string,
): Buffer {
  retargetAtaPayer(poolTx, raise, relayer);
  let tx = fillFee > 0 ? prependFillFee(poolTx, raise, feeVault, fillFee) : poolTx;
  tx.feePayer = relayer;
  tx.recentBlockhash = blockhash;
  const message = tx.serializeMessage();
  if (message.length > PACKET_LIMIT) {
    throw new Error(`launch message ${message.length} B exceeds ${PACKET_LIMIT}`);
  }
  if (message[0] & 0x80) {
    throw new Error("versioned / ALT message is not allowed");
  }
  return Buffer.from(message);
}

/** Same SDK path as createPoolWithFirstBuy, but builds config+pool locally (no live config). */
export async function buildSdkLaunchTransaction(args: {
  connection: Connection;
  relayer: PublicKey;
  raise: PublicKey;
  feeVault: PublicKey;
  mint: PublicKey;
  config: PublicKey;
  name: string;
  symbol: string;
  uri: string;
  buyLamports: bigint;
}): Promise<Transaction> {
  const client = new DynamicBondingCurveClient(args.connection, "confirmed");
  const curve = demoCurveConfig();
  const combo = await client.partner.createConfigAndPoolWithFirstBuy({
    payer: args.relayer,
    config: args.config,
    feeClaimer: args.feeVault,
    leftoverReceiver: args.raise,
    quoteMint: NATIVE_MINT,
    transferFeeParameters: null,
    ...curve,
    preCreatePoolParam: {
      baseMint: args.mint,
      name: args.name,
      symbol: args.symbol,
      uri: args.uri,
      poolCreator: args.raise,
    },
    firstBuyParam: {
      buyer: args.raise,
      buyAmount: new BN(args.buyLamports.toString()),
      minimumAmountOut: new BN(1),
      referralTokenAccount: null,
    },
  });
  return combo.createPoolWithFirstBuyTx;
}

export async function buildLaunchMessage(): Promise<Record<string, unknown>> {
  const predecessor =
    process.env.CONTRACT_ID ?? process.env.NEAR_PREDECESSOR ?? "example.testnet";
  const trustId = process.env.TRUST_ID ?? "1";
  const config = process.env.DBC_CONFIG;
  if (!config) {
    throw new Error("Set DBC_CONFIG to the pinned createConfig2 pubkey.");
  }
  const credited = BigInt(process.env.CREDITED_LAMPORTS ?? "0");
  if (credited <= 0n) {
    throw new Error("Set CREDITED_LAMPORTS to the settled raise total.");
  }
  const name = process.env.TOKEN_NAME ?? "Keyless";
  const symbol = process.env.TOKEN_SYMBOL ?? "KLT";
  const uri = process.env.TOKEN_URI ?? "https://ethics.ltd/k.json";
  const alreadyPaid = process.env.FILL_FEE_ALREADY_PAID === "1";
  const fee = fillFeeLamports(credited);
  const buy = firstBuyLamports(credited);
  if (buy <= 0n) {
    throw new Error("first-buy amount must be > 0");
  }

  const rows = await deriveTrustAddresses({
    predecessor,
    paths: [`trust/${trustId}/raise`, `trust/${trustId}/fees`, `trust/${trustId}/mint`],
  });
  const raise = new PublicKey(
    rows.find((r) => r.path.endsWith("/raise"))!.solana_address,
  );
  const fees = new PublicKey(
    rows.find((r) => r.path.endsWith("/fees"))!.solana_address,
  );
  const mint = new PublicKey(
    rows.find((r) => r.path.endsWith("/mint"))!.solana_address,
  );
  const relayer = loadRelayerKeypair();

  const connection = new Connection(RPC, "confirmed");
  const client = new DynamicBondingCurveClient(connection, "confirmed");
  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  let poolTx = await client.creator.createPoolWithFirstBuy({
    createPoolParam: {
      baseMint: mint,
      config: new PublicKey(config),
      name,
      symbol,
      uri,
      payer: relayer.publicKey,
      poolCreator: raise,
    },
    firstBuyParam: {
      buyer: raise,
      buyAmount: new BN(buy.toString()),
      minimumAmountOut: new BN(1),
      referralTokenAccount: null,
    },
  });
  const message = finalizeLaunchMessage(
    poolTx,
    raise,
    fees,
    relayer.publicKey,
    alreadyPaid ? 0 : Number(fee),
    blockhash,
  );

  return {
    trustId,
    predecessor,
    raise: raise.toBase58(),
    fees: fees.toBase58(),
    mint: mint.toBase58(),
    config,
    name,
    symbol,
    uri,
    paramsHash: await pinParamsHash(config, name, symbol, uri),
    creditedLamports: credited.toString(),
    fillFeeLamports: alreadyPaid ? "0" : fee.toString(),
    buyLamports: buy.toString(),
    minOut: "1",
    messageBytes: message.length,
    packetLimit: PACKET_LIMIT,
    messageBase64: Buffer.from(message).toString("base64"),
    messageHash: createHash("sha256").update(message).digest("hex"),
    relayerFeePayer: relayer.publicKey.toBase58(),
    recentBlockhash: blockhash,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const report = await buildLaunchMessage();
  console.log(JSON.stringify(report, null, 2));
}
