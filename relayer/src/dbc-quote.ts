/**
 * SDK 2.0.0 exact-in quote (quote → base) for a brand-new pool.
 * `eligibleForFirstSwapWithMinFee` follows the config flag: the launch
 * builder includes the instructions sysvar when that flag is set, and the
 * on-chain program then charges the scheduler minimum.
 */
import BN from "bn.js";
import {
  SwapMode,
  getQuoteFromInputAmount,
} from "@meteora-ag/dynamic-bonding-curve-sdk";
import type { StoredCurve } from "./launch-math.ts";
import type { demoCurveConfig } from "./dbc-curve.ts";

type CurveConfig = ReturnType<typeof demoCurveConfig>;

function bn(v: BN | number | string): string {
  return new BN(v).toString(10);
}

export function storedCurveFromConfig(config: CurveConfig): StoredCurve {
  const points = config.curve
    .filter((p) => !p.sqrtPrice.isZero() && !p.liquidity.isZero())
    .map((p) => ({
      sqrtPrice: bn(p.sqrtPrice),
      liquidity: bn(p.liquidity),
    }));
  return {
    sqrtStartPrice: bn(config.sqrtStartPrice),
    migrationQuoteThreshold: bn(config.migrationQuoteThreshold),
    collectFeeMode: config.collectFeeMode,
    enableFirstSwapWithMinFee: Boolean(config.enableFirstSwapWithMinFee),
    baseFeeMode: config.poolFees.baseFee.baseFeeMode,
    cliffFeeNumerator: bn(config.poolFees.baseFee.cliffFeeNumerator),
    numberOfPeriod: config.poolFees.baseFee.firstFactor,
    reductionFactor: bn(config.poolFees.baseFee.thirdFactor),
    points,
  };
}

export function quoteFirstBuyOutput(config: CurveConfig, amountIn: BN): BN {
  const quoted = getQuoteFromInputAmount({
    config,
    amountIn,
    swapBaseForQuote: false,
    swapMode: SwapMode.ExactIn,
    hasReferral: false,
    slippageBps: 0,
    eligibleForFirstSwapWithMinFee: Boolean(config.enableFirstSwapWithMinFee),
  });
  return quoted.outputAmount;
}
