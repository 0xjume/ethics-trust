import { demoCurveConfig } from "./dbc-curve.ts";
import { storedCurveFromConfig } from "./dbc-quote.ts";
import { asBigInt } from "./near-json.ts";
import type { StoredCurve } from "./launch-math.ts";

export type RawCurve = {
  sqrt_start_price: number | string;
  migration_quote_threshold: number | string;
  collect_fee_mode: number;
  enable_first_swap_with_min_fee: boolean;
  base_fee_mode: number;
  cliff_fee_numerator: number | string;
  number_of_period: number;
  reduction_factor: number | string;
  points?: Array<{ sqrt_price: number | string; liquidity: number | string }>;
};

let demo: StoredCurve | undefined;

function demoStored(): StoredCurve {
  demo ??= storedCurveFromConfig(demoCurveConfig());
  return demo;
}

function same(left: number | string, right: string): boolean {
  return asBigInt(left) === asBigInt(right);
}

/**
 * The launch builder quotes `demoCurveConfig`. The contract quotes the curve
 * stored on the trust. Launch only when those curves are the same, so
 * `minimum_amount_out` is the contract's number.
 */
export function curveMatchesBuilder(curve: RawCurve): boolean {
  const expected = demoStored();
  const points = curve.points ?? [];
  if (points.length !== expected.points.length) return false;
  return (
    same(curve.sqrt_start_price, expected.sqrtStartPrice) &&
    same(curve.migration_quote_threshold, expected.migrationQuoteThreshold) &&
    curve.collect_fee_mode === expected.collectFeeMode &&
    Boolean(curve.enable_first_swap_with_min_fee) === expected.enableFirstSwapWithMinFee &&
    curve.base_fee_mode === expected.baseFeeMode &&
    same(curve.cliff_fee_numerator, expected.cliffFeeNumerator) &&
    curve.number_of_period === expected.numberOfPeriod &&
    same(curve.reduction_factor, expected.reductionFactor) &&
    points.every(
      (point, index) =>
        same(point.sqrt_price, expected.points[index]!.sqrtPrice) &&
        same(point.liquidity, expected.points[index]!.liquidity),
    )
  );
}
