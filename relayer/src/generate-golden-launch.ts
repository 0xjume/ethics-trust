/**
 * Build one honest SDK 2.0.0 Shape C message (combo createPoolWithFirstBuy)
 * with seeded keys and write the fixture the Rust golden test loads.
 *
 * Needs Solana devnet RPC only for ATA existence checks + a blockhash.
 * Does not send.
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { DBC_PROGRAM } from "./dbc-tx-size.ts";
import { buildSdkLaunchTransaction, finalizeLaunchMessage } from "./build-launch-message.ts";

const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
const SWAP_DISC = Buffer.from([248, 198, 158, 145, 225, 117, 135, 200]);
const SWAP2_DISC = Buffer.from([65, 75, 63, 76, 235, 91, 91, 136]);

function seeded(tag: number): Keypair {
  return Keypair.fromSeed(Buffer.alloc(32, tag));
}

export async function generateGoldenLaunchFixture() {
  const relayer = seeded(8);
  const raise = seeded(1);
  const mint = seeded(2);
  const fee = seeded(3);
  const config = seeded(4);
  const name = "Demo";
  const symbol = "DEMO";
  const uri = "https://example.com/m.json";
  const buy = 985n;
  const fillFee = 15;

  const connection = new Connection(RPC, "confirmed");
  const { blockhash } = await connection.getLatestBlockhash("confirmed");
  const poolTx = await buildSdkLaunchTransaction({
    connection,
    relayer: relayer.publicKey,
    raise: raise.publicKey,
    feeVault: fee.publicKey,
    mint: mint.publicKey,
    config: config.publicKey,
    name,
    symbol,
    uri,
    buyLamports: buy,
  });
  const message = finalizeLaunchMessage(
    poolTx,
    raise.publicKey,
    fee.publicKey,
    relayer.publicKey,
    fillFee,
    blockhash,
  );

  const swap = poolTx.instructions.find((ix) => {
    const d = Buffer.from(ix.data);
    return d.subarray(0, 8).equals(SWAP_DISC) || d.subarray(0, 8).equals(SWAP2_DISC);
  });
  if (!swap) {
    throw new Error("SDK launch tx has no swap instruction");
  }

  const fixture = {
    sdk: "@meteora-ag/dynamic-bonding-curve-sdk@2.0.0",
    dbcProgram: DBC_PROGRAM,
    name,
    symbol,
    uri,
    buyLamports: buy.toString(),
    fillFeeLamports: fillFee.toString(),
    minOut: "1",
    relayer: relayer.publicKey.toBase58(),
    raise: raise.publicKey.toBase58(),
    mint: mint.publicKey.toBase58(),
    fee: fee.publicKey.toBase58(),
    config: config.publicKey.toBase58(),
    recentBlockhash: blockhash,
    messageBytes: message.length,
    messageBase64: message.toString("base64"),
    swapProgram: swap.programId.toBase58(),
    swapAccountCount: swap.keys.length,
    swapAccounts: swap.keys.map((k) => k.pubkey.toBase58()),
    swapDataLen: swap.data.length,
    swapDataHex: Buffer.from(swap.data).toString("hex"),
    versioned: false,
  };
  return fixture;
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const fixture = await generateGoldenLaunchFixture();
  const out = join(
    dirname(fileURLToPath(import.meta.url)),
    "..",
    "..",
    "contracts",
    "keyless-trust",
    "tests",
    "fixtures",
    "sdk_launch.json",
  );
  writeFileSync(out, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(JSON.stringify({ wrote: out, ...fixture }, null, 2));
}
