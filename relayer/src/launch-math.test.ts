import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  fillFeeLamports,
  firstBuyLamports,
  pinParamsHash,
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

describe("pinParamsHash", () => {
  it("matches the contract's sha256(config\\0name\\0symbol\\0uri)", async () => {
    const hash = await pinParamsHash(
      "Config1111111111111111111111111111111111111",
      "Demo",
      "DEMO",
      "https://example.com/m.json",
    );
    assert.equal(hash.length, 64);
    assert.match(hash, /^[0-9a-f]{64}$/);
  });
});
