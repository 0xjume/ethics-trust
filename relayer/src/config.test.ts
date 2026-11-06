import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Keypair } from "@solana/web3.js";
import { KeyPair } from "near-api-js";
import { DEFAULT_CONTRACT_ID, DEFAULT_POLL_MS, loadConfig } from "./config.ts";
import { encodeBase58 } from "./keypair.ts";

describe("loadConfig", () => {
  it("exits the setup with a clear error when keys are missing", () => {
    assert.throws(
      () => loadConfig({}),
      /NEAR_WATCHER_ACCOUNT_ID/,
    );
  });

  it("defaults to the m10 contract, a 12s poll, and dry-run", () => {
    const near = KeyPair.fromRandom("ed25519").toString();
    const solana = Keypair.generate();
    const config = loadConfig({
      NEAR_WATCHER_ACCOUNT_ID: "watcher.testnet",
      NEAR_WATCHER_PRIVATE_KEY: near,
      SOLANA_RELAYER_KEYPAIR: encodeBase58(solana.secretKey),
      HEALTH_DISABLE: "1",
    });
    assert.equal(config.contractId, DEFAULT_CONTRACT_ID);
    assert.equal(config.pollMs, DEFAULT_POLL_MS);
    assert.equal(config.solanaSubmit, false);
    assert.equal(config.relayer.publicKey.equals(solana.publicKey), true);
    assert.equal(config.healthPort, null);
  });

  it("refuses a mainnet RPC", () => {
    const near = KeyPair.fromRandom("ed25519").toString();
    assert.throws(
      () =>
        loadConfig({
          NEAR_WATCHER_ACCOUNT_ID: "watcher.testnet",
          NEAR_WATCHER_PRIVATE_KEY: near,
          SOLANA_RELAYER_KEYPAIR: encodeBase58(Keypair.generate().secretKey),
          SOLANA_RPC_URL: "https://api.mainnet-beta.solana.com",
          HEALTH_DISABLE: "1",
        }),
      /devnet/,
    );
  });
});
