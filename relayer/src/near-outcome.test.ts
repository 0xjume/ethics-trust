import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { nearCallResult } from "./near-outcome.ts";

describe("nearCallResult", () => {
  it("reads the transaction hash and the final success value", () => {
    const payload = Buffer.from(
      JSON.stringify({ Ed25519: { signature: [1, 2, 3] } }),
    ).toString("base64");
    const result = nearCallResult({
      transaction_outcome: { id: "NearTx" },
      status: { SuccessValue: payload },
    });
    assert.equal(result.txHash, "NearTx");
    assert.deepEqual(result.value, { Ed25519: { signature: [1, 2, 3] } });
  });

  it("throws when the receipt failed", () => {
    assert.throws(
      () =>
        nearCallResult({
          transaction_outcome: { id: "NearTx" },
          status: { Failure: { ActionError: { kind: "FunctionCallError" } } },
        }),
      /NEAR call failed/,
    );
  });
});
