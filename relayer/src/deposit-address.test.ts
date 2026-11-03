import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { depositPath, localDepositAddressView } from "./deposit-address.ts";

describe("deposit-address", () => {
  it("builds trust/<id>/in/<pubkey>", () => {
    assert.equal(
      depositPath(1, "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj"),
      "trust/1/in/B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj",
    );
  });

  it("returns the MPC view inputs the contract would", () => {
    const view = localDepositAddressView({
      trustId: 1,
      contributor: "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj",
      predecessor: "keyless-trust-dev-54ef71.testnet",
    });
    assert.equal(view.path, "trust/1/in/B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj");
    assert.equal(view.predecessor, "keyless-trust-dev-54ef71.testnet");
    assert.equal(view.mpc_contract, "v1.signer-prod.testnet");
    assert.equal(view.domain_id, 1);
  });
});
