import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Keypair } from "@solana/web3.js";
import {
  DEFAULT_MPC_SIGN_TGAS,
  buildRelayerPaidVaultTransfer,
  resolveAttachedTgas,
} from "./near-signed-solana-transfer.ts";

describe("relayer-paid vault transfer", () => {
  it("defaults attached gas to 30 TGas and stays configurable", () => {
    assert.equal(DEFAULT_MPC_SIGN_TGAS, 30);
    assert.equal(resolveAttachedTgas({}), 30);
    assert.equal(resolveAttachedTgas({ MPC_SIGN_TGAS: "30" }), 30);
    assert.equal(resolveAttachedTgas({ MPC_SIGN_TGAS: "40" }), 40);
  });

  it("uses a local relayer as fee payer so the vault is not account 0", () => {
    const relayer = Keypair.generate();
    const vault = Keypair.generate();
    const destination = Keypair.generate();
    const tx = buildRelayerPaidVaultTransfer({
      relayer: relayer.publicKey,
      vault: vault.publicKey,
      destination: destination.publicKey,
      lamports: 5000,
      blockhash: Keypair.generate().publicKey.toBase58(),
    });

    const message = tx.compileMessage();
    const bytes = message.serialize();
    assert.equal(message.version, "legacy");
    assert.equal(tx.feePayer?.equals(relayer.publicKey), true);
    assert.equal(message.accountKeys[0].equals(relayer.publicKey), true);
    assert.equal(message.accountKeys[0].equals(vault.publicKey), false);
    assert.ok(message.accountKeys.some((key) => key.equals(vault.publicKey)));
    assert.equal(message.header.numRequiredSignatures, 2);
    assert.ok(bytes.length < 1232);
    assert.equal(message.instructions.length, 1);
  });

  it("rejects using the vault as its own fee payer", () => {
    const vault = Keypair.generate();
    assert.throws(() =>
      buildRelayerPaidVaultTransfer({
        relayer: vault.publicKey,
        vault: vault.publicKey,
        destination: Keypair.generate().publicKey,
        lamports: 1,
        blockhash: Keypair.generate().publicKey.toBase58(),
      }),
    );
  });
});
