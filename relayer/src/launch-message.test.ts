import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Keypair, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import { leftoverReceiverIsRaise } from "./create-dbc-config.ts";
import { PACKET_LIMIT } from "./dbc-tx-size.ts";
import { prependFillFee } from "./build-launch-message.ts";

describe("Shape C builders", () => {
  it("pins leftoverReceiver to the raise vault", () => {
    assert.equal(leftoverReceiverIsRaise(), true);
  });

  it("prepends the fill-fee transfer and stays under 1232 bytes", () => {
    const raise = Keypair.generate().publicKey;
    const fee = Keypair.generate().publicKey;
    const relayer = Keypair.generate().publicKey;
    const dummy = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: raise,
        toPubkey: new PublicKey("11111111111111111111111111111111"),
        lamports: 1,
      }),
    );
    dummy.feePayer = relayer;
    dummy.recentBlockhash = "11111111111111111111111111111111";
    const withFee = prependFillFee(dummy, raise, fee, 15);
    const serialized = withFee.serializeMessage();
    assert.equal(withFee.instructions[0].programId.equals(SystemProgram.programId), true);
    assert.ok(serialized.length < PACKET_LIMIT);
    assert.equal(serialized[0] & 0x80, 0);
  });
});
