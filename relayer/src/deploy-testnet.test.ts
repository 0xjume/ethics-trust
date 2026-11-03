import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { KeyPair } from "near-api-js";
import {
  DEPLOY_WAIT_UNTIL,
  defaultWasmPath,
  deployAccountActions,
  keepDeployFullAccessKey,
  resolveDeployTarget,
  storageDepositNear,
} from "./deploy-testnet.ts";

describe("resolveDeployTarget", () => {
  it("builds a subaccount of the throwaway testnet parent", () => {
    const t = resolveDeployTarget({
      NEAR_NETWORK: "testnet",
      NEAR_ACCOUNT_ID: "keyless-trust-dev-54ef71.testnet",
      DEPLOY_SUBACCOUNT: "m2",
    });
    assert.equal(t.contractId, "m2.keyless-trust-dev-54ef71.testnet");
    assert.equal(t.wasmPath, defaultWasmPath());
    assert.match(t.wasmPath, /target\/wasm32-unknown-unknown\/release\/keyless_trust\.wasm$/);
    assert.doesNotMatch(t.wasmPath, /\/relayer\/target\//);
  });

  it("refuses mainnet", () => {
    assert.throws(
      () => resolveDeployTarget({ NEAR_NETWORK: "mainnet" }),
      /must be testnet/,
    );
    assert.throws(
      () =>
        resolveDeployTarget({
          NEAR_NETWORK: "testnet",
          NEAR_ACCOUNT_ID: "something.near",
        }),
      /mainnet/,
    );
  });
});

describe("storageDepositNear", () => {
  it("floors at 5.5 NEAR so 405 KB wasm is not under-funded", () => {
    assert.equal(storageDepositNear(405_499), 5.5);
    assert.equal(storageDepositNear(221_000), 5.5);
  });

  it("grows with larger wasm", () => {
    assert.ok(storageDepositNear(800_000) > 5.5);
  });
});

describe("deployAccountActions", () => {
  const publicKey = KeyPair.fromRandom("ed25519").getPublicKey();
  const wasm = new Uint8Array([0, 1, 2]);

  it("omits the parent full-access key by default", () => {
    assert.equal(keepDeployFullAccessKey({}), false);
    const acts = deployAccountActions({
      wasm,
      depositNear: 5.5,
      publicKey,
      keepFullAccessKey: false,
      mpcContract: null,
    });
    assert.equal(acts.length, 4);
  });

  it("waits for FINAL before returning", () => {
    assert.equal(DEPLOY_WAIT_UNTIL, "FINAL");
  });

  it("adds the key only when DEPLOY_KEEP_KEY=1", () => {
    assert.equal(keepDeployFullAccessKey({ DEPLOY_KEEP_KEY: "1" }), true);
    const acts = deployAccountActions({
      wasm,
      depositNear: 5.5,
      publicKey,
      keepFullAccessKey: true,
      mpcContract: null,
    });
    assert.equal(acts.length, 5);
  });
});
