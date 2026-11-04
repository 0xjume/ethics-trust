/**
 * Build (do not send) createConfig2 + createPoolWithFirstBuy as legacy txs.
 * Dummy pubkeys. No address-lookup tables. Reports serialized message size vs 1232.
 */
import { NATIVE_MINT } from "@solana/spl-token";
import { Connection, Keypair, PublicKey, Transaction } from "@solana/web3.js";
import { DynamicBondingCurveClient } from "@meteora-ag/dynamic-bonding-curve-sdk";
import BN from "bn.js";
import { demoCurveConfig } from "./dbc-curve.ts";

export const DBC_PROGRAM = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
export const PACKET_LIMIT = 1232;

const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

function measureTx(label: string, tx: Transaction) {
  if (tx.instructions.length === 0) {
    throw new Error(`${label}: empty transaction`);
  }
  const hasLookupTables =
    "addressTableLookups" in tx.compileMessage() &&
    (tx.compileMessage() as { addressTableLookups?: unknown[] }).addressTableLookups
      ?.length;
  const message = tx.compileMessage();
  const serialized = message.serialize();
  const version = message.version;
  return {
    label,
    version,
    hasLookupTables: Boolean(hasLookupTables),
    instructionCount: tx.instructions.length,
    accountCount: message.accountKeys.length,
    serializedMessageBytes: serialized.length,
    packetLimit: PACKET_LIMIT,
    remainingBytes: PACKET_LIMIT - serialized.length,
    fitsLegacyPacket: serialized.length <= PACKET_LIMIT,
    programIds: [
      ...new Set(tx.instructions.map((ix) => ix.programId.toBase58())),
    ],
  };
}

function dummy(): PublicKey {
  return Keypair.generate().publicKey;
}

export async function buildDbcSizeReport() {
  const connection = new Connection(RPC, "confirmed");
  const client = new DynamicBondingCurveClient(connection, "confirmed");
  const { blockhash } = await connection.getLatestBlockhash("confirmed");

  const payer = dummy();
  const config = dummy();
  const feeVault = dummy();
  const raiseVault = dummy();
  const baseMint = dummy();
  const curve = demoCurveConfig();

  const createConfigTx = await client.partner.createConfig2({
    payer,
    config,
    feeClaimer: feeVault,
    leftoverReceiver: raiseVault,
    quoteMint: NATIVE_MINT,
    transferFeeParameters: null,
    ...curve,
  });
  createConfigTx.feePayer = payer;
  createConfigTx.recentBlockhash = blockhash;

  const reports = [measureTx("createConfig2", createConfigTx)];

  let poolNote =
    "createPoolWithFirstBuy built against dummy pubkeys (no on-chain config)";
  try {
    const poolTx = await client.creator.createPoolWithFirstBuy({
      createPoolParam: {
        baseMint,
        config,
        name: "Keyless",
        symbol: "KLT",
        uri: "https://ethics.ltd/k.json",
        payer,
        poolCreator: raiseVault,
      },
      firstBuyParam: {
        buyer: raiseVault,
        buyAmount: new BN(2_000_000_000),
        minimumAmountOut: new BN(1),
        referralTokenAccount: null,
      },
    });
    poolTx.feePayer = raiseVault;
    poolTx.recentBlockhash = blockhash;
    reports.push(measureTx("createPoolWithFirstBuy", poolTx));
  } catch (error) {
    poolNote = `createPoolWithFirstBuy needs on-chain config (${
      error instanceof Error ? error.message : error
    }); measuring createConfigAndPoolWithFirstBuy instead`;
    const combo = await client.partner.createConfigAndPoolWithFirstBuy({
      payer: raiseVault,
      config,
      feeClaimer: feeVault,
      leftoverReceiver: raiseVault,
      quoteMint: NATIVE_MINT,
      transferFeeParameters: null,
      ...curve,
      preCreatePoolParam: {
        baseMint,
        name: "Keyless",
        symbol: "KLT",
        uri: "https://ethics.ltd/k.json",
        poolCreator: raiseVault,
      },
      firstBuyParam: {
        buyer: raiseVault,
        buyAmount: new BN(2_000_000_000),
        minimumAmountOut: new BN(1),
        referralTokenAccount: null,
      },
    });
    combo.createConfigTx.feePayer = raiseVault;
    combo.createConfigTx.recentBlockhash = blockhash;
    combo.createPoolWithFirstBuyTx.feePayer = raiseVault;
    combo.createPoolWithFirstBuyTx.recentBlockhash = blockhash;
    reports.push(measureTx("createConfigAndPoolWithFirstBuy.config", combo.createConfigTx));
    reports.push(
      measureTx("createConfigAndPoolWithFirstBuy.pool+buy", combo.createPoolWithFirstBuyTx),
    );
  }

  const program = await connection.getAccountInfo(new PublicKey(DBC_PROGRAM));
  return {
    rpc: RPC,
    sdk: "@meteora-ag/dynamic-bonding-curve-sdk@2.0.0",
    dbcProgram: DBC_PROGRAM,
    dbcProgramDeployed: Boolean(program?.executable),
    dbcProgramOwner: program?.owner.toBase58() ?? null,
    dbcProgramLamports: program?.lamports ?? null,
    dummyPubkeys: true,
    addressLookupTables: false,
    note: poolNote,
    transactions: reports,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const report = await buildDbcSizeReport();
  console.log(JSON.stringify(report, null, 2));
}
