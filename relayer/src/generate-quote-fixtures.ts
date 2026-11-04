/**
 * SDK 2.0.0 exact-in quote (quote → base) for a brand-new pool.
 * Writes the fixture the Rust quote tests load. Does not send.
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import BN from "bn.js";
import {
  ActivationType,
  BaseFeeMode,
  CollectFeeMode,
  MigrationFeeOption,
  MigrationOption,
  TokenAuthorityOption,
  TokenDecimal,
  TokenType,
  buildCurve,
} from "@meteora-ag/dynamic-bonding-curve-sdk";
import { quoteFirstBuyOutput, storedCurveFromConfig } from "./dbc-quote.ts";
import { pinParamsHash, type StoredCurve } from "./launch-math.ts";

type CurveInput = Parameters<typeof buildCurve>[0];
type CurveConfig = ReturnType<typeof buildCurve>;

function input(fee: CurveInput["fee"]): CurveInput {
  return {
    token: {
      tokenType: TokenType.SPLToken,
      tokenBaseDecimal: TokenDecimal.SIX,
      tokenQuoteDecimal: TokenDecimal.NINE,
      tokenAuthorityOption: TokenAuthorityOption.Immutable,
      totalTokenSupply: 1_000_000_000,
      leftover: 0,
    },
    fee,
    migration: {
      migrationOption: MigrationOption.MET_DAMM_V2,
      migrationFeeOption: MigrationFeeOption.FixedBps200,
      migrationFee: { feePercentage: 0, creatorFeePercentage: 0 },
    },
    liquidityDistribution: {
      partnerLiquidityPercentage: 0,
      partnerPermanentLockedLiquidityPercentage: 100,
      creatorLiquidityPercentage: 0,
      creatorPermanentLockedLiquidityPercentage: 0,
    },
    lockedVesting: {
      totalLockedVestingAmount: 0,
      numberOfVestingPeriod: 0,
      cliffUnlockAmount: 0,
      totalVestingDuration: 0,
      cliffDurationFromMigrationTime: 0,
    },
    activationType: ActivationType.Timestamp,
    percentageSupplyOnMigration: 20,
    migrationQuoteThreshold: 10,
  };
}

const flatFee = {
  baseFeeParams: {
    baseFeeMode: BaseFeeMode.FeeSchedulerLinear,
    feeSchedulerParam: {
      startingFeeBps: 100,
      endingFeeBps: 100,
      numberOfPeriod: 0,
      totalDuration: 0,
    },
  },
  dynamicFeeEnabled: true,
  collectFeeMode: CollectFeeMode.QuoteToken,
  creatorTradingFeePercentage: 50,
  poolCreationFee: 0,
  enableFirstSwapWithMinFee: true,
} satisfies CurveInput["fee"];

const decayingFee = {
  ...flatFee,
  baseFeeParams: {
    baseFeeMode: BaseFeeMode.FeeSchedulerLinear,
    feeSchedulerParam: {
      startingFeeBps: 500,
      endingFeeBps: 100,
      numberOfPeriod: 4,
      totalDuration: 400,
    },
  },
} satisfies CurveInput["fee"];

function bn(v: BN | number | string): string {
  return new BN(v).toString(10);
}

function pack(name: string, config: CurveConfig, amounts: string[]) {
  const stored = storedCurveFromConfig(config);
  const quotes = amounts.map((amountIn) => ({
    amountIn,
    outputAmount: bn(quoteFirstBuyOutput(config, new BN(amountIn))),
  }));
  return {
    name,
    ...stored,
    periodFrequency: bn(config.poolFees.baseFee.secondFactor),
    quotes,
  };
}

export async function buildQuoteFixtures(): Promise<{
  sdk: string;
  demoParamsHash: string;
  curves: Array<StoredCurve & { name: string; periodFrequency: string; quotes: { amountIn: string; outputAmount: string }[] }>;
}> {
  const amounts = ["1000000", "985000000", "2000000000", "8500000000"];
  const curves = [
    pack("demo", buildCurve(input(flatFee)), amounts),
    pack(
      "output-fee",
      buildCurve(input({ ...flatFee, collectFeeMode: CollectFeeMode.OutputToken })),
      ["1000000000"],
    ),
    pack("decaying-min-fee", buildCurve(input(decayingFee)), ["1000000000"]),
    pack(
      "cliff-not-min",
      buildCurve(input({ ...decayingFee, enableFirstSwapWithMinFee: false })),
      ["1000000000"],
    ),
  ];
  const demo = curves[0];
  const demoParamsHash = await pinParamsHash(
    "Config1111111111111111111111111111111111111",
    "Demo",
    "DEMO",
    "https://example.com/m.json",
    demo,
  );
  return {
    sdk: "@meteora-ag/dynamic-bonding-curve-sdk@2.0.0",
    demoParamsHash,
    curves,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const fixture = await buildQuoteFixtures();
  const out = join(
    dirname(fileURLToPath(import.meta.url)),
    "..",
    "..",
    "contracts",
    "keyless-trust",
    "tests",
    "fixtures",
    "dbc_quotes.json",
  );
  writeFileSync(out, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(JSON.stringify({ wrote: out, demoParamsHash: fixture.demoParamsHash }, null, 2));
}
