import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  borshStoredCurve,
  fillFeeLamports,
  firstBuyLamports,
  pinParamsHash,
  type StoredCurve,
} from "./launch-math.ts";

describe("launch amounts", () => {
  it("takes 1.5% and leaves the rest for the first buy", () => {
    assert.equal(fillFeeLamports(1000n), 15n);
    assert.equal(firstBuyLamports(1000n), 985n);
  });

  it("is zero when there is nothing credited", () => {
    assert.equal(fillFeeLamports(0n), 0n);
    assert.equal(firstBuyLamports(0n), 0n);
  });
});

const curve: StoredCurve = {
  sqrtStartPrice: "32609544831759054",
  migrationQuoteThreshold: "10000000000",
  collectFeeMode: 0,
  enableFirstSwapWithMinFee: true,
  baseFeeMode: 0,
  cliffFeeNumerator: "10000000",
  numberOfPeriod: 0,
  reductionFactor: "0",
  points: [
    {
      sqrtPrice: "130438178253327822",
      liquidity: "34783514296328165318017769454091",
    },
  ],
};

describe("pinParamsHash", () => {
  it("appends borsh(curve) after config\\0name\\0symbol\\0uri", async () => {
    const hash = await pinParamsHash(
      "Config1111111111111111111111111111111111111",
      "Demo",
      "DEMO",
      "https://example.com/m.json",
      curve,
    );
    const other = await pinParamsHash(
      "Config1111111111111111111111111111111111111",
      "Demo",
      "DEMO",
      "https://example.com/m.json",
      { ...curve, cliffFeeNumerator: "20000000" },
    );
    assert.equal(hash.length, 64);
    assert.match(hash, /^[0-9a-f]{64}$/);
    assert.notEqual(hash, other);
    assert.ok(borshStoredCurve(curve).length > 16);
  });
});
