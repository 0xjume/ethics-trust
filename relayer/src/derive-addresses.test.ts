import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { solanaAddressFromNearKey } from "./near-rpc.ts";

describe("solanaAddressFromNearKey", () => {
  it("strips the ed25519 prefix used by NEAR public keys", () => {
    assert.equal(
      solanaAddressFromNearKey(
        "ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj",
      ),
      "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj",
    );
  });
});
