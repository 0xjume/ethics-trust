import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Message, PublicKey } from "@solana/web3.js";
import { PACKET_LIMIT } from "./dbc-tx-size.ts";
import { goldenDistributeFixture } from "./generate-golden-distribute.ts";

describe("Shape D builder", () => {
  it("golden fixture is the spl-token message and stays under 1232 bytes", () => {
    const path = join(
      dirname(fileURLToPath(import.meta.url)),
      "..",
      "..",
      "contracts",
      "keyless-trust",
      "tests",
      "fixtures",
      "sdk_distribute.json",
    );
    const onDisk = JSON.parse(readFileSync(path, "utf8")) as {
      messageBase64: string;
      messageBytes: number;
      decimals: number;
      tokenProgram: string;
      legs: { amount: string; createAta: boolean }[];
    };
    const fresh = goldenDistributeFixture();
    assert.equal(fresh.messageBase64, onDisk.messageBase64);
    assert.equal(onDisk.decimals, 6);
    assert.equal(onDisk.tokenProgram, TOKEN_PROGRAM_ID.toBase58());
    assert.equal(onDisk.legs.length, 2);
    assert.equal(onDisk.legs[0].amount, "33");
    assert.equal(onDisk.legs[1].amount, "66");
    assert.ok(onDisk.messageBytes <= PACKET_LIMIT);
    const msg = Message.from(Buffer.from(onDisk.messageBase64, "base64"));
    assert.equal(msg.header.numRequiredSignatures, 2);
    assert.equal(msg.serialize()[0] & 0x80, 0);
    assert.equal(msg.instructions.length, 4);
    const mint = new PublicKey(fresh.mint);
    const raise = new PublicKey(fresh.raise);
    assert.ok(msg.accountKeys.some((k) => k.equals(mint)));
    assert.ok(msg.accountKeys.some((k) => k.equals(raise)));
    assert.ok(msg.accountKeys[0].equals(new PublicKey(fresh.relayer)));
  });
});
