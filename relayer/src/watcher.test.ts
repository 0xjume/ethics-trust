import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { confirmTransferArgs } from "./watcher.ts";

describe("confirmTransferArgs", () => {
  it("maps to the contract confirm_transfer args", () => {
    assert.deepEqual(
      confirmTransferArgs({
        messageHash: "ab",
        solanaSig: "sig",
      }),
      { message_hash: "ab", solana_sig: "sig" },
    );
  });
});
