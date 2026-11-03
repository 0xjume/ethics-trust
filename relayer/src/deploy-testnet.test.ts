import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveDeployTarget } from "./deploy-testnet.ts";

describe("resolveDeployTarget", () => {
  it("builds a subaccount of the throwaway testnet parent", () => {
    const t = resolveDeployTarget({
      NEAR_NETWORK: "testnet",
      NEAR_ACCOUNT_ID: "keyless-trust-dev-54ef71.testnet",
      DEPLOY_SUBACCOUNT: "m2",
    });
    assert.equal(t.contractId, "m2.keyless-trust-dev-54ef71.testnet");
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
