/**
 * Read the contract and the deposit balances the planner needs.
 * Amounts that the contract computes (`sweepable`, `refundable`,
 * `claimable`) come from views, so rent close-out and live locks match.
 */
import type { PlanInput, TrustPlan } from "./driver.ts";
import type { TrustState } from "./driver.ts";
import { bookedDepositBalance } from "./policy.ts";
import { asBigInt } from "./near-json.ts";
import type { RawCurve } from "./curve.ts";

export type RawTrust = {
  id: number | string;
  state: TrustState;
  watcher: string;
  target_lamports: number | string;
  credited_total: number | string;
  deadline_ns: number | string;
  execution_timeout_ns: number | string;
  filled_at_ns: number | string | null;
  execution_started_ns: number | string | null;
  fill_fee_bps: number;
  fill_fee_paid: number | string;
  raise_address: string | null;
  fee_address: string | null;
  mint_address: string | null;
  dbc_config: string;
  name: string;
  symbol: string;
  uri: string;
  curve: RawCurve;
};

export type RawContribution = {
  contributor: string;
  deposit_address: string | null;
  reported_lamports: number | string;
  credited_lamports: number | string;
  overflow_lamports: number | string;
  swept_lamports: number | string;
  refunded_lamports: number | string;
  last_solana_sig: string;
};

export type ChainReads = {
  nowNs(): Promise<bigint>;
  listTrusts(): Promise<RawTrust[]>;
  listContributorKeys(): Promise<Array<{ trustId: string; contributor: string }>>;
  getContribution(trustId: string, contributor: string): Promise<RawContribution | null>;
  sweepable(trustId: string, contributor: string): Promise<bigint>;
  refundable(trustId: string, contributor: string): Promise<bigint>;
  raiseRefundable(trustId: string, contributor: string): Promise<bigint>;
  claimable(trustId: string, contributor: string): Promise<bigint>;
  listPending(): Promise<PlanInput["pendings"]>;
  getBalance(address: string): Promise<bigint>;
  signaturesFor(address: string): Promise<string[]>;
  curveMatches(curve: RawCurve): boolean;
};

function optTime(value: number | string | null): bigint | null {
  if (value == null) return null;
  return asBigInt(value);
}

export async function gatherPlan(
  chain: ChainReads,
  memory: { timeoutAttempted: ReadonlySet<string>; burnedSignatures: ReadonlySet<string> },
  watcherAccountId: string,
  trustFilter: string | null,
): Promise<PlanInput> {
  const [nowNs, trusts, keys, pendings] = await Promise.all([
    chain.nowNs(),
    chain.listTrusts(),
    chain.listContributorKeys(),
    chain.listPending(),
  ]);
  const selected = trusts.filter((trust) => trustFilter == null || String(trust.id) === trustFilter);
  const planned: TrustPlan[] = [];
  for (const trust of selected) {
    const id = String(trust.id);
    const rows = keys.filter((key) => key.trustId === id);
    const contributors = await Promise.all(
      rows.map(async (key) => {
        const row = await chain.getContribution(id, key.contributor);
        if (!row) return null;
        const reported = asBigInt(row.reported_lamports);
        const swept = asBigInt(row.swept_lamports);
        const refunded = asBigInt(row.refunded_lamports);
        const address = row.deposit_address;
        const balance = address ? await chain.getBalance(address) : 0n;
        const booked = bookedDepositBalance({ reported, swept, refunded });
        const signatures =
          address && balance > booked ? await chain.signaturesFor(address) : [];
        const [sweepable, refundable, raiseRefundable, claimable] = await Promise.all([
          chain.sweepable(id, key.contributor),
          chain.refundable(id, key.contributor),
          chain.raiseRefundable(id, key.contributor),
          chain.claimable(id, key.contributor),
        ]);
        return {
          contributor: row.contributor,
          depositAddress: address,
          reported,
          credited: asBigInt(row.credited_lamports),
          overflow: asBigInt(row.overflow_lamports),
          swept,
          refunded,
          lastSolanaSig: row.last_solana_sig ?? "",
          balance,
          signatures,
          sweepable,
          refundable,
          raiseRefundable,
          claimable,
        };
      }),
    );
    const present = contributors.filter((row) => row != null);
    const raiseBalance = trust.raise_address ? await chain.getBalance(trust.raise_address) : 0n;
    planned.push({
      id,
      state: trust.state,
      watcher: trust.watcher,
      targetLamports: asBigInt(trust.target_lamports),
      creditedTotal: asBigInt(trust.credited_total),
      deadlineNs: asBigInt(trust.deadline_ns),
      executionTimeoutNs: asBigInt(trust.execution_timeout_ns),
      filledAtNs: optTime(trust.filled_at_ns),
      executionStartedNs: optTime(trust.execution_started_ns),
      fillFeeBps: trust.fill_fee_bps,
      fillFeePaid: asBigInt(trust.fill_fee_paid),
      sweptTotal: present.reduce((sum, row) => sum + row.swept, 0n),
      raiseAddress: trust.raise_address,
      feeAddress: trust.fee_address,
      mintAddress: trust.mint_address,
      dbcConfig: trust.dbc_config,
      name: trust.name,
      symbol: trust.symbol,
      uri: trust.uri,
      curveMatchesBuilder: chain.curveMatches(trust.curve),
      raiseBalance,
      contributors: present,
    });
  }
  return {
    nowNs,
    watcherAccountId,
    trusts: planned,
    pendings,
    timeoutAttempted: memory.timeoutAttempted,
    burnedSignatures: memory.burnedSignatures,
  };
}
