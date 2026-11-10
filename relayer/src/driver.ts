/**
 * One tick of the watcher. Pure: no RPC. The contract still rejects a
 * message whose destination is not a stored address. This planner only
 * names addresses that already sit on the trust or the contributor row.
 * It does not set `tokens_bought`.
 */
import type { DepositState } from "./deposit-decision.ts";
import { decideDepositReport, unreportedLamports } from "./deposit-decision.ts";
import { MAX_DISTRIBUTE_BATCH } from "./distribute-math.ts";
import { fillFeeLamports, safeTransferAmount } from "./policy.ts";

export type TrustState = DepositState;

export type ContributorPlan = {
  contributor: string;
  depositAddress: string | null;
  reported: bigint;
  credited: bigint;
  overflow: bigint;
  swept: bigint;
  refunded: bigint;
  lastSolanaSig: string;
  balance: bigint;
  signatures: readonly string[];
  sweepable: bigint;
  refundable: bigint;
  raiseRefundable: bigint;
  claimable: bigint;
};

export type PendingPlan = {
  trustId: string;
  contributor: string;
  kind: string;
  messageHash: string;
  expiresAtNs: bigint;
  settled: boolean;
  released: boolean;
  superseded: boolean;
};

export type TrustPlan = {
  id: string;
  state: TrustState;
  watcher: string;
  targetLamports: bigint;
  creditedTotal: bigint;
  deadlineNs: bigint;
  executionTimeoutNs: bigint;
  filledAtNs: bigint | null;
  executionStartedNs: bigint | null;
  fillFeeBps: number;
  fillFeePaid: bigint;
  sweptTotal: bigint;
  raiseAddress: string | null;
  feeAddress: string | null;
  mintAddress: string | null;
  dbcConfig: string;
  name: string;
  symbol: string;
  uri: string;
  /** Stored curve matches the SDK curve the launch builder quotes. */
  curveMatchesBuilder: boolean;
  raiseBalance: bigint;
  contributors: ContributorPlan[];
};

export type PlanInput = {
  nowNs: bigint;
  watcherAccountId: string;
  trusts: TrustPlan[];
  pendings: PendingPlan[];
  timeoutAttempted: ReadonlySet<string>;
  burnedSignatures?: ReadonlySet<string>;
};

export type PlannedAction =
  | { type: "try_timeout"; trustId: string; state: TrustState }
  | {
      type: "report_deposit";
      trustId: string;
      state: TrustState;
      contributor: string;
      lamports: bigint;
      solanaSig: string;
    }
  | {
      type: "release_pending";
      trustId: string;
      state: TrustState;
      messageHash: string;
      kind: string;
    }
  | {
      type: "request_refund";
      trustId: string;
      state: TrustState;
      contributor: string;
      lamports: bigint;
      source: string;
      destination: string;
    }
  | {
      type: "request_raise_refund";
      trustId: string;
      state: TrustState;
      contributor: string;
      lamports: bigint;
      source: string;
      destination: string;
    }
  | {
      type: "request_sweep";
      trustId: string;
      state: TrustState;
      contributor: string;
      lamports: bigint;
      source: string;
      destination: string;
    }
  | {
      type: "request_fill_fee";
      trustId: string;
      state: TrustState;
      lamports: bigint;
      source: string;
      destination: string;
    }
  | {
      type: "request_launch";
      trustId: string;
      state: TrustState;
      raise: string;
      fee: string;
      mint: string;
      dbcConfig: string;
      name: string;
      symbol: string;
      uri: string;
      buyLamports: bigint;
    }
  | {
      type: "request_distribute";
      trustId: string;
      state: TrustState;
      raise: string;
      mint: string;
      legs: Array<{ owner: string; amount: bigint }>;
    };

export type PlanNote = {
  trustId: string;
  state: TrustState;
  reason: string;
};

export type PlanResult = {
  actions: PlannedAction[];
  notes: PlanNote[];
};

function needsTimeout(trust: TrustPlan, nowNs: bigint): boolean {
  if (trust.state === "Refunding") return true;
  if (
    trust.state === "Raising" &&
    nowNs >= trust.deadlineNs &&
    trust.creditedTotal < trust.targetLamports
  ) {
    return true;
  }
  if (
    trust.state === "Filled" &&
    trust.filledAtNs != null &&
    nowNs >= trust.filledAtNs + trust.executionTimeoutNs
  ) {
    return true;
  }
  if (
    trust.state === "Executing" &&
    trust.executionStartedNs != null &&
    nowNs >= trust.executionStartedNs + trust.executionTimeoutNs
  ) {
    return true;
  }
  return false;
}

function lockBlocks(pending: PendingPlan): boolean {
  return !pending.settled && !pending.released && !pending.superseded;
}

function blocked(
  pendings: PendingPlan[],
  trustId: string,
  contributor: string,
  kind: string,
): boolean {
  return pendings.some(
    (row) =>
      row.trustId === trustId &&
      row.contributor === contributor &&
      row.kind === kind &&
      lockBlocks(row),
  );
}

function exactAmount(balance: bigint, amount: bigint): bigint | null {
  const safe = safeTransferAmount(balance, amount);
  return safe === amount ? amount : null;
}

export function planTick(input: PlanInput): PlanResult {
  const actions: PlannedAction[] = [];
  const releases: PlannedAction[] = [];
  const notes: PlanNote[] = [];
  const burned = input.burnedSignatures ?? new Set<string>();

  for (const trust of input.trusts) {
    const state = trust.state;
    const ours = trust.watcher === input.watcherAccountId;
    if (!ours) {
      notes.push({
        trustId: trust.id,
        state,
        reason: "named watcher is a different account; this process will not report or confirm",
      });
    }
    const timingOut = needsTimeout(trust, input.nowNs);
    if (timingOut && !input.timeoutAttempted.has(trust.id)) {
      actions.push({ type: "try_timeout", trustId: trust.id, state });
    }
    const effective: TrustState = timingOut ? "Refunding" : state;

    if (ours) {
      for (const row of trust.contributors) {
        if (!row.depositAddress) {
          if (row.balance > 0n) {
            notes.push({
              trustId: trust.id,
              state,
              reason: `deposit address for ${row.contributor} is not derived yet`,
            });
          }
          continue;
        }
        const decision = decideDepositReport({
          state: effective,
          balance: row.balance,
          reported: row.reported,
          swept: row.swept,
          refunded: row.refunded,
          lastSolanaSig: row.lastSolanaSig,
          signatures: row.signatures,
          burned,
        });
        const ahead = unreportedLamports({
          balance: row.balance,
          reported: row.reported,
          swept: row.swept,
          refunded: row.refunded,
        });
        if (!decision && ahead > 0n && effective !== "Launched" && effective !== "Distributed") {
          notes.push({
            trustId: trust.id,
            state,
            reason: `deposit balance for ${row.contributor} is ahead of the ledger but no unused Solana signature is available`,
          });
        }
        if (decision) {
          actions.push({
            type: "report_deposit",
            trustId: trust.id,
            state,
            contributor: row.contributor,
            lamports: decision.lamports,
            solanaSig: decision.solanaSig,
          });
        }
      }
    }

    const reported = actions.some(
      (action) => action.type === "report_deposit" && action.trustId === trust.id,
    );
    if (ours && !reported) {
      const relay = planRelay(trust, effective, input.pendings, notes);
      if (relay) actions.push(relay);
    }
  }

  for (const pending of input.pendings) {
    if (pending.settled || pending.released || pending.superseded) continue;
    if (input.nowNs < pending.expiresAtNs) continue;
    const trust = input.trusts.find((row) => row.id === pending.trustId);
    releases.push({
      type: "release_pending",
      trustId: pending.trustId,
      state: trust?.state ?? "Raising",
      messageHash: pending.messageHash,
      kind: pending.kind,
    });
  }

  return { actions: [...releases, ...actions], notes };
}

function planRelay(
  trust: TrustPlan,
  state: TrustState,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (state === "Refunding" || state === "Raising") {
    return planRefund(trust, state, pendings, notes);
  }
  if (state === "Filled" || state === "Executing") {
    const refund = planOverflowRefund(trust, state, pendings, notes);
    if (refund) return refund;
    const sweep = planSweep(trust, state, pendings, notes);
    if (sweep) return sweep;
    if (state === "Executing") return planLaunch(trust, pendings, notes);
  }
  if (state === "Launched") return planDistribute(trust, pendings, notes);
  return null;
}

function planRefund(
  trust: TrustPlan,
  state: TrustState,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (state === "Raising") return planOverflowRefund(trust, state, pendings, notes);
  for (const row of trust.contributors) {
    if (row.refundable <= 0n || !row.depositAddress) continue;
    if (blocked(pendings, trust.id, row.contributor, "refund")) continue;
    const amount = exactAmount(row.balance, row.refundable);
    if (amount == null) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: `refund for ${row.contributor} does not match the live deposit balance`,
      });
      continue;
    }
    return {
      type: "request_refund",
      trustId: trust.id,
      state: trust.state,
      contributor: row.contributor,
      lamports: amount,
      source: row.depositAddress,
      destination: row.contributor,
    };
  }
  if (!trust.raiseAddress) return null;
  for (const row of trust.contributors) {
    if (row.raiseRefundable <= 0n) continue;
    if (blocked(pendings, trust.id, row.contributor, "raise_refund")) continue;
    const amount = exactAmount(trust.raiseBalance, row.raiseRefundable);
    if (amount == null) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: `raise refund for ${row.contributor} does not match the raise vault balance`,
      });
      continue;
    }
    return {
      type: "request_raise_refund",
      trustId: trust.id,
      state: trust.state,
      contributor: row.contributor,
      lamports: amount,
      source: trust.raiseAddress,
      destination: row.contributor,
    };
  }
  return null;
}

function planOverflowRefund(
  trust: TrustPlan,
  state: TrustState,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (state !== "Raising" && state !== "Filled" && state !== "Executing") return null;
  for (const row of trust.contributors) {
    if (row.refundable <= 0n || !row.depositAddress) continue;
    if (blocked(pendings, trust.id, row.contributor, "refund")) continue;
    const amount = exactAmount(row.balance, row.refundable);
    if (amount == null) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: `overflow refund for ${row.contributor} does not match the live deposit balance`,
      });
      continue;
    }
    return {
      type: "request_refund",
      trustId: trust.id,
      state: trust.state,
      contributor: row.contributor,
      lamports: amount,
      source: row.depositAddress,
      destination: row.contributor,
    };
  }
  return null;
}

function planSweep(
  trust: TrustPlan,
  state: TrustState,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (!trust.raiseAddress) {
    if (trust.contributors.some((row) => row.sweepable > 0n)) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: "raise address is not derived; sweep skipped",
      });
    }
    return null;
  }
  for (const row of trust.contributors) {
    if (row.sweepable <= 0n || !row.depositAddress) continue;
    if (blocked(pendings, trust.id, row.contributor, "sweep")) continue;
    const amount = safeTransferAmount(row.balance, row.sweepable);
    if (amount == null || amount <= 0n) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: `sweep for ${row.contributor} would leave the deposit account below rent`,
      });
      continue;
    }
    return {
      type: "request_sweep",
      trustId: trust.id,
      state: trust.state,
      contributor: row.contributor,
      lamports: amount,
      source: row.depositAddress,
      destination: trust.raiseAddress,
    };
  }
  return null;
}

function planLaunch(
  trust: TrustPlan,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (trust.creditedTotal <= 0n || trust.sweptTotal !== trust.creditedTotal) return null;
  if (trust.contributors.some((row) => blocked(pendings, trust.id, row.contributor, "sweep"))) {
    return null;
  }
  if (blocked(pendings, trust.id, "", "fill_fee") || blocked(pendings, trust.id, "", "launch")) {
    return null;
  }
  const fee = fillFeeLamports(trust.creditedTotal, trust.fillFeeBps);
  if (trust.fillFeePaid < fee) {
    if (trust.fillFeePaid > 0n) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: "fill fee is partially paid; the contract accepts one transfer for the full fee",
      });
      return null;
    }
    if (!trust.raiseAddress || !trust.feeAddress) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: "fill fee is unpaid and a vault address is missing",
      });
      return null;
    }
    const amount = exactAmount(trust.raiseBalance, fee);
    if (amount == null) {
      notes.push({
        trustId: trust.id,
        state: trust.state,
        reason: "raise vault balance cannot cover the exact fill fee",
      });
      return null;
    }
    return {
      type: "request_fill_fee",
      trustId: trust.id,
      state: trust.state,
      lamports: amount,
      source: trust.raiseAddress,
      destination: trust.feeAddress,
    };
  }
  if (!trust.curveMatchesBuilder) {
    notes.push({
      trustId: trust.id,
      state: trust.state,
      reason:
        "stored curve does not match the demo curve the launch builder quotes; refusing to invent minimum_amount_out",
    });
    return null;
  }
  if (!trust.raiseAddress || !trust.feeAddress || !trust.mintAddress || !trust.dbcConfig) {
    notes.push({
      trustId: trust.id,
      state: trust.state,
      reason: "launch vault, mint, or DBC config is missing",
    });
    return null;
  }
  const buy = trust.creditedTotal - fee;
  if (buy <= 0n) return null;
  return {
    type: "request_launch",
    trustId: trust.id,
    state: trust.state,
    raise: trust.raiseAddress,
    fee: trust.feeAddress,
    mint: trust.mintAddress,
    dbcConfig: trust.dbcConfig,
    name: trust.name,
    symbol: trust.symbol,
    uri: trust.uri,
    buyLamports: buy,
  };
}

function planDistribute(
  trust: TrustPlan,
  pendings: PendingPlan[],
  notes: PlanNote[],
): PlannedAction | null {
  if (!trust.raiseAddress || !trust.mintAddress) {
    notes.push({
      trustId: trust.id,
      state: trust.state,
      reason: "distribute needs the stored raise and mint addresses",
    });
    return null;
  }
  const legs: Array<{ owner: string; amount: bigint }> = [];
  for (const row of trust.contributors) {
    if (row.claimable <= 0n) continue;
    if (blocked(pendings, trust.id, row.contributor, "distribute")) continue;
    legs.push({ owner: row.contributor, amount: row.claimable });
    if (legs.length === MAX_DISTRIBUTE_BATCH) break;
  }
  if (legs.length === 0) return null;
  return {
    type: "request_distribute",
    trustId: trust.id,
    state: trust.state,
    raise: trust.raiseAddress,
    mint: trust.mintAddress,
    legs,
  };
}
