import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  defaultWasmPath,
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
