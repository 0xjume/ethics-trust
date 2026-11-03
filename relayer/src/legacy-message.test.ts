import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { PublicKey, SystemProgram, Transaction } from "@solana/web3.js";

describe("legacy Solana transfer message", () => {
  it("serializes under the 1232-byte packet limit", () => {
    const from = PublicKey.unique();
    const to = PublicKey.unique();
    const tx = new Transaction();
    tx.feePayer = from;
    tx.recentBlockhash = PublicKey.unique().toBase58();
    tx.add(SystemProgram.transfer({ fromPubkey: from, toPubkey: to, lamports: 1 }));
    const message = tx.compileMessage();
    const bytes = message.serialize();
    assert.equal(message.version, "legacy");
    assert.ok(bytes.length < 1232);
    assert.equal(message.instructions.length, 1);
  });
});
