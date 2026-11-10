import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { decideDepositReport, unreportedLamports } from "./deposit-decision.ts";

const SIG = "S".repeat(88);

describe("unreportedLamports", () => {
  it("is the balance above reported minus swept and refunded", () => {
    assert.equal(
      unreportedLamports({ balance: 1_500n, reported: 1_000n, swept: 400n, refunded: 100n }),
      1_000n,
    );
    assert.equal(
      unreportedLamports({ balance: 500n, reported: 1_000n, swept: 400n, refunded: 100n }),
      0n,
    );
    assert.equal(
      unreportedLamports({ balance: 0n, reported: 0n, swept: 0n, refunded: 0n }),
      0n,
    );
  });
});

describe("decideDepositReport", () => {
  const row = {
    balance: 500n,
    reported: 0n,
    swept: 0n,
    refunded: 0n,
    lastSolanaSig: "",
    signatures: [SIG],
  };

  it("reports the delta while the trust can still take deposits", () => {
    const decision = decideDepositReport({ state: "Raising", ...row });
    assert.deepEqual(decision, { lamports: 500n, solanaSig: SIG });
  });

  it("does not report after launch", () => {
    assert.equal(decideDepositReport({ state: "Launched", ...row }), null);
    assert.equal(decideDepositReport({ state: "Distributed", ...row }), null);
  });

  it("does not invent a signature", () => {
    assert.equal(
      decideDepositReport({ state: "Raising", ...row, signatures: [] }),
      null,
    );
    assert.equal(
      decideDepositReport({ state: "Filled", ...row, signatures: ["short"] }),
      null,
    );
  });

  it("skips a signature already stored on the row or burned", () => {
    assert.equal(
      decideDepositReport({ state: "Executing", ...row, lastSolanaSig: SIG }),
      null,
    );
    assert.equal(
      decideDepositReport({
        state: "Refunding",
        ...row,
        burned: new Set([SIG]),
      }),
      null,
    );
  });

  it("uses the first unused signature", () => {
    const older = "A".repeat(87);
    const decision = decideDepositReport({
      state: "Raising",
      ...row,
      signatures: [SIG, older],
      lastSolanaSig: SIG,
    });
    assert.equal(decision?.solanaSig, older);
  });
});
