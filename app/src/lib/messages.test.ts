import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Keypair, Message, PublicKey } from "@solana/web3.js";
import { buildDistributeMessage, buildRefundMessage } from "./messages.ts";

const BLOCKHASH = "11111111111111111111111111111111";

describe("Solana messages the contract can parse", () => {
  it("builds a legacy refund with the wallet as fee payer and the deposit as source", () => {
    const feePayer = Keypair.generate().publicKey;
    const source = Keypair.generate().publicKey;
    const bytes = buildRefundMessage({
      feePayer,
      source,
      destination: feePayer,
      lamports: 5_000_000n,
      blockhash: BLOCKHASH,
    });
    assert.equal(bytes[0]! & 0x80, 0);
    const message = Message.from(bytes);
    assert.equal(message.header.numRequiredSignatures, 2);
    assert.equal(message.accountKeys[0]?.toBase58(), feePayer.toBase58());
    assert.ok(message.accountKeys.some((key) => key.equals(source)));
    assert.ok(message.instructions.length >= 1);
  });

  it("builds a two-signer distribute message that does not let the raise pay rent", () => {
    const feePayer = Keypair.generate().publicKey;
    const raise = Keypair.generate().publicKey;
    const mint = Keypair.generate().publicKey;
    const owner = Keypair.generate().publicKey;
    const bytes = buildDistributeMessage({
      feePayer,
      raise,
      mint,
      blockhash: BLOCKHASH,
      legs: [{ owner, amount: 1_000_000n }],
    });
    const message = Message.from(bytes);
    assert.equal(message.header.numRequiredSignatures, 2);
    assert.equal(message.accountKeys[0]?.toBase58(), feePayer.toBase58());
    assert.notEqual(message.accountKeys[0]?.toBase58(), raise.toBase58());
    assert.ok(message.accountKeys.some((key) => key.equals(new PublicKey(mint))));
    assert.ok(bytes.length < 1232);
  });
});
