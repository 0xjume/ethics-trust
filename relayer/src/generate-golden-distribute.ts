/**
 * Deterministic Shape D fixture (no RPC). Seeded on-curve keys, fixed
 * blockhash, @solana/spl-token TransferChecked + CreateIdempotent.
 * The Rust golden test loads the written JSON.
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Keypair } from "@solana/web3.js";
import { buildDistributeMessageBytes } from "./build-distribute-message.ts";
import { TOKEN_DECIMALS } from "./distribute-math.ts";

const BLOCKHASH = "11111111111111111111111111111111";

function seeded(tag: number): Keypair {
  return Keypair.fromSeed(Buffer.alloc(32, tag));
}

export function goldenDistributeFixture() {
  const relayer = seeded(8);
  const raise = seeded(1);
  const mint = seeded(2);
  const fee = seeded(3);
  const owners = [seeded(11), seeded(12)];
  const amounts = [33n, 66n];
  const legs = owners.map((kp, i) => ({
    owner: kp.publicKey,
    amount: amounts[i]!,
    createAta: true,
  }));
  const message = buildDistributeMessageBytes({
    relayer: relayer.publicKey,
    raise: raise.publicKey,
    mint: mint.publicKey,
    legs,
    blockhash: BLOCKHASH,
  });
  return {
    sdk: "@solana/spl-token@0.4.13 + @solana/web3.js@1",
    tokenProgram: TOKEN_PROGRAM_ID.toBase58(),
    ataProgram: ASSOCIATED_TOKEN_PROGRAM_ID.toBase58(),
    decimals: TOKEN_DECIMALS,
    relayer: relayer.publicKey.toBase58(),
    raise: raise.publicKey.toBase58(),
    mint: mint.publicKey.toBase58(),
    fee: fee.publicKey.toBase58(),
    recentBlockhash: BLOCKHASH,
    legs: legs.map((leg) => ({
      owner: leg.owner.toBase58(),
      amount: leg.amount.toString(),
      createAta: leg.createAta,
    })),
    messageBytes: message.length,
    messageBase64: message.toString("base64"),
    versioned: false,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const fixture = goldenDistributeFixture();
  const out = join(
    dirname(fileURLToPath(import.meta.url)),
    "..",
    "..",
    "contracts",
    "keyless-trust",
    "tests",
    "fixtures",
    "sdk_distribute.json",
  );
  writeFileSync(out, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(JSON.stringify({ wrote: out, messageBytes: fixture.messageBytes }, null, 2));
}
