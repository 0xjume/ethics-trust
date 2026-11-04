import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { MAX_TOKENS_BOUGHT, proRataShare } from "./distribute-math.ts";

describe("pro-rata share", () => {
  it("floors and leaves dust in the raise ATA", () => {
    assert.equal(proRataShare(0n, 1n, 1n), 0n);
    assert.equal(proRataShare(100n, 0n, 100n), 0n);
    assert.equal(proRataShare(1_000n, 1n, 2n), 500n);
    const a = proRataShare(100n, 1n, 3n);
    const b = proRataShare(100n, 2n, 3n);
    assert.equal(a, 33n);
    assert.equal(b, 66n);
    assert.equal(100n - a - b, 1n);
    assert.equal(proRataShare(MAX_TOKENS_BOUGHT, 1n, 1n), MAX_TOKENS_BOUGHT);
  });
});
