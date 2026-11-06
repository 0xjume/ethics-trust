import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { curveMatchesBuilder } from "./curve.ts";
import { demoCurveConfig } from "./dbc-curve.ts";
import { storedCurveFromConfig } from "./dbc-quote.ts";

describe("curveMatchesBuilder", () => {
  it("accepts the demo curve and rejects a different start price", () => {
    const stored = storedCurveFromConfig(demoCurveConfig());
    const raw = {
      sqrt_start_price: stored.sqrtStartPrice,
      migration_quote_threshold: stored.migrationQuoteThreshold,
      collect_fee_mode: stored.collectFeeMode,
      enable_first_swap_with_min_fee: stored.enableFirstSwapWithMinFee,
      base_fee_mode: stored.baseFeeMode,
      cliff_fee_numerator: stored.cliffFeeNumerator,
      number_of_period: stored.numberOfPeriod,
      reduction_factor: stored.reductionFactor,
      points: stored.points.map((point) => ({
        sqrt_price: point.sqrtPrice,
        liquidity: point.liquidity,
      })),
    };
    assert.equal(curveMatchesBuilder(raw), true);
    assert.equal(curveMatchesBuilder({ ...raw, sqrt_start_price: "1" }), false);
  });
});