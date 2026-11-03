import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Keypair, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import { ASSOCIATED_TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { leftoverReceiverIsRaise } from "./create-dbc-config.ts";
import { PACKET_LIMIT } from "./dbc-tx-size.ts";
import { prependFillFee, retargetAtaPayer } from "./build-launch-message.ts";

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

  it("retargets ATA CreateIdempotent payer from raise to relayer", () => {
    const raise = Keypair.generate().publicKey;
    const relayer = Keypair.generate().publicKey;
    const ata = Keypair.generate().publicKey;
    const mint = Keypair.generate().publicKey;
    const tx = new Transaction().add({
      programId: ASSOCIATED_TOKEN_PROGRAM_ID,
      keys: [
        { pubkey: raise, isSigner: true, isWritable: true },
        { pubkey: ata, isSigner: false, isWritable: true },
        { pubkey: raise, isSigner: false, isWritable: false },
        { pubkey: mint, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: raise, isSigner: false, isWritable: false },
      ],
      data: Buffer.from([1]),
    });
    retargetAtaPayer(tx, raise, relayer);
    assert.ok(tx.instructions[0].keys[0].pubkey.equals(relayer));
    assert.equal(tx.instructions[0].keys[0].isSigner, true);
    assert.ok(tx.instructions[0].keys[2].pubkey.equals(raise));
  });
});
