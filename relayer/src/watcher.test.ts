import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { confirmTransferArgs, watchDeposits } from "./watcher.ts";

const SIG = "S".repeat(88);

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

describe("watchDeposits", () => {
  it("reads the Solana balance and reports the unreported delta", async () => {
    const seen: string[] = [];
    const rows = await watchDeposits(
      [
        {
          trustId: "4",
          state: "Raising",
          contributor: "ContributorPubkey111111111111111111111",
          depositAddress: "DepositPubkey1111111111111111111111111",
          reported: 100n,
          swept: 0n,
          refunded: 0n,
          lastSolanaSig: "",
          signatures: [SIG],
        },
      ],
      {
        async getBalance(address) {
          seen.push(address);
          return 350n;
        },
      },
    );
    assert.deepEqual(seen, ["DepositPubkey1111111111111111111111111"]);
    assert.equal(rows[0]?.report, true);
    assert.equal(rows[0]?.depositLamports, 350n);
    assert.equal(rows[0]?.unreportedLamports, 250n);
    assert.equal(rows[0]?.solanaSig, SIG);
  });

  it("does not report a balance that has no unused signature", async () => {
    const rows = await watchDeposits(
      [
        {
          trustId: "4",
          state: "Raising",
          contributor: "ContributorPubkey111111111111111111111",
          depositAddress: "DepositPubkey1111111111111111111111111",
          reported: 0n,
          swept: 0n,
          refunded: 0n,
          lastSolanaSig: "",
          signatures: [],
        },
      ],
      { async getBalance() { return 10n; } },
    );
    assert.equal(rows[0]?.report, false);
    assert.equal(rows[0]?.unreportedLamports, 10n);
    assert.equal(rows[0]?.solanaSig, null);
  });
});
