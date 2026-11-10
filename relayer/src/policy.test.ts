import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { RENT_EXEMPT_MIN, safeTransferAmount } from "./policy.ts";

describe("safeTransferAmount", () => {
  it("closes the account when the whole balance moves", () => {
    assert.equal(safeTransferAmount(1_000n, 1_000n), 1_000n);
  });

  it("sends a partial amount when the leftover stays rent-exempt", () => {
    const balance = RENT_EXEMPT_MIN + 50_000n;
    assert.equal(safeTransferAmount(balance, 50_000n), 50_000n);
  });

  it("shrinks a sweep that would leave dust below rent", () => {
    const balance = 2_000_000n;
    const desired = 1_500_000n;
    assert.equal(safeTransferAmount(balance, desired), balance - RENT_EXEMPT_MIN);
  });

  it("refuses a partial close of an account that is already below rent", () => {
    assert.equal(safeTransferAmount(1_000n, 100n), null);
  });
});
