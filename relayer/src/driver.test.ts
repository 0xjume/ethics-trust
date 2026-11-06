import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { planTick, type ContributorPlan, type TrustPlan } from "./driver.ts";

const WATCHER = "watcher.testnet";
const SIG = "S".repeat(88);
const NOW = 10_000n;

function contributor(partial: Partial<ContributorPlan> = {}): ContributorPlan {
  return {
    contributor: "ContributorPubkey111111111111111111111",
    depositAddress: "DepositPubkey1111111111111111111111111",
    reported: 0n,
    credited: 0n,
    overflow: 0n,
    swept: 0n,
    refunded: 0n,
    lastSolanaSig: "",
    balance: 0n,
    signatures: [],
    sweepable: 0n,
    refundable: 0n,
    raiseRefundable: 0n,
    claimable: 0n,
    ...partial,
  };
}

function trust(partial: Partial<TrustPlan> = {}): TrustPlan {
  return {
    id: "1",
    state: "Raising",
    watcher: WATCHER,
    targetLamports: 10_000n,
    creditedTotal: 0n,
    deadlineNs: 50_000n,
    executionTimeoutNs: 5_000n,
    filledAtNs: null,
    executionStartedNs: null,
    fillFeeBps: 150,
    fillFeePaid: 0n,
    sweptTotal: 0n,
    raiseAddress: "RaisePubkey111111111111111111111111111",
    feeAddress: "FeePubkey11111111111111111111111111111",
    mintAddress: "MintPubkey1111111111111111111111111111",
    dbcConfig: "ConfigPubkey11111111111111111111111111",
    name: "Keyless",
    symbol: "KLT",
    uri: "https://ethics.ltd/k.json",
    curveMatchesBuilder: true,
    raiseBalance: 0n,
    contributors: [],
    ...partial,
  };
}

function plan(partial: Partial<TrustPlan>, extra: Partial<Parameters<typeof planTick>[0]> = {}) {
  return planTick({
    nowNs: NOW,
    watcherAccountId: WATCHER,
    trusts: [trust(partial)],
    pendings: [],
    timeoutAttempted: new Set(),
    ...extra,
  });
}

describe("planTick", () => {
  it("reports a deposit before it sweeps", () => {
    const result = plan({
      state: "Filled",
      creditedTotal: 10_000n,
      sweptTotal: 0n,
      contributors: [
        contributor({
          reported: 0n,
          credited: 0n,
          balance: 10_000n,
          signatures: [SIG],
          sweepable: 0n,
        }),
      ],
    });
    assert.equal(result.actions.length, 1);
    assert.equal(result.actions[0]?.type, "report_deposit");
    if (result.actions[0]?.type === "report_deposit") {
      assert.equal(result.actions[0].lamports, 10_000n);
      assert.equal(result.actions[0].solanaSig, SIG);
    }
  });

  it("calls try_timeout when the deadline has passed under target", () => {
    const result = plan({
      state: "Raising",
      deadlineNs: 1_000n,
      creditedTotal: 100n,
      targetLamports: 10_000n,
    });
    assert.equal(result.actions[0]?.type, "try_timeout");
  });

  it("does not call try_timeout again after a successful attempt", () => {
    const result = plan(
      { state: "Refunding", creditedTotal: 100n },
      { timeoutAttempted: new Set(["1"]) },
    );
    assert.equal(
      result.actions.some((action) => action.type === "try_timeout"),
      false,
    );
  });

  it("refunds the full refundable balance in Refunding", () => {
    const result = plan({
      state: "Refunding",
      creditedTotal: 1_000n,
      contributors: [
        contributor({
          balance: 1_000n,
          refundable: 1_000n,
          credited: 1_000n,
        }),
      ],
    });
    const refund = result.actions.find((action) => action.type === "request_refund");
    assert.ok(refund && refund.type === "request_refund");
    assert.equal(refund.lamports, 1_000n);
    assert.equal(refund.destination, refund.contributor);
    assert.equal(refund.source, "DepositPubkey1111111111111111111111111");
  });

  it("sweeps at most the sweepable amount into the stored raise address", () => {
    const result = plan({
      state: "Filled",
      creditedTotal: 2_000_000n,
      contributors: [
        contributor({
          reported: 2_000_000n,
          credited: 2_000_000n,
          balance: 2_000_000n,
          sweepable: 2_000_000n,
        }),
      ],
    });
    const sweep = result.actions.find((action) => action.type === "request_sweep");
    assert.ok(sweep && sweep.type === "request_sweep");
    assert.equal(sweep.lamports, 2_000_000n);
    assert.equal(sweep.destination, "RaisePubkey111111111111111111111111111");
  });

  it("does not sweep while a sweep lock is still held", () => {
    const result = plan(
      {
        state: "Executing",
        creditedTotal: 2_000_000n,
        contributors: [
          contributor({
            reported: 2_000_000n,
            balance: 2_000_000n,
            sweepable: 2_000_000n,
          }),
        ],
      },
      {
        pendings: [
          {
            trustId: "1",
            contributor: "ContributorPubkey111111111111111111111",
            kind: "sweep",
            messageHash: "ab".repeat(32),
            expiresAtNs: NOW + 1n,
            settled: false,
            released: false,
            superseded: false,
          },
        ],
      },
    );
    assert.equal(
      result.actions.some((action) => action.type === "request_sweep"),
      false,
    );
  });

  it("releases an expired pending before asking for another signature", () => {
    const result = plan(
      { state: "Executing", creditedTotal: 1n, sweptTotal: 1n, fillFeePaid: 0n },
      {
        pendings: [
          {
            trustId: "1",
            contributor: "",
            kind: "launch",
            messageHash: "cd".repeat(32),
            expiresAtNs: NOW,
            settled: false,
            released: false,
            superseded: false,
          },
        ],
      },
    );
    assert.equal(result.actions[0]?.type, "release_pending");
    assert.equal(
      result.actions.some((action) => action.type === "request_launch"),
      false,
    );
  });

  it("pays the fill fee once sweeps match credits", () => {
    const result = plan({
      state: "Executing",
      creditedTotal: 2_000_000n,
      sweptTotal: 2_000_000n,
      fillFeeBps: 150,
      fillFeePaid: 0n,
      raiseBalance: 2_000_000n,
    });
    const fee = result.actions.find((action) => action.type === "request_fill_fee");
    assert.ok(fee && fee.type === "request_fill_fee");
    assert.equal(fee.lamports, 30_000n);
    assert.equal(fee.destination, "FeePubkey11111111111111111111111111111");
  });

  it("requests launch after the fill fee is paid", () => {
    const result = plan({
      state: "Executing",
      creditedTotal: 10_000n,
      sweptTotal: 10_000n,
      fillFeePaid: 150n,
      raiseBalance: 9_850n,
    });
    const launch = result.actions.find((action) => action.type === "request_launch");
    assert.ok(launch && launch.type === "request_launch");
    assert.equal(launch.buyLamports, 9_850n);
    assert.equal(launch.raise, "RaisePubkey111111111111111111111111111");
  });

  it("does not launch when the stored curve is not the builder curve", () => {
    const result = plan({
      state: "Executing",
      creditedTotal: 10_000n,
      sweptTotal: 10_000n,
      fillFeePaid: 150n,
      curveMatchesBuilder: false,
    });
    assert.equal(
      result.actions.some((action) => action.type === "request_launch"),
      false,
    );
    assert.match(result.notes[0]?.reason ?? "", /minimum_amount_out/);
  });

  it("batches at most five distribute legs", () => {
    const contributors = Array.from({ length: 6 }, (_, index) =>
      contributor({
        contributor: `Owner${index}Pubkey111111111111111111111111`,
        claimable: BigInt(index + 1),
      }),
    );
    const result = plan({
      state: "Launched",
      creditedTotal: 10_000n,
      sweptTotal: 10_000n,
      contributors,
    });
    const distribute = result.actions.find((action) => action.type === "request_distribute");
    assert.ok(distribute && distribute.type === "request_distribute");
    assert.equal(distribute.legs.length, 5);
    assert.equal(distribute.legs[0]?.amount, 1n);
  });

  it("does not act for a trust whose watcher is someone else, except timeout", () => {
    const result = plan({
      state: "Filled",
      watcher: "other.testnet",
      creditedTotal: 2_000_000n,
      contributors: [
        contributor({
          reported: 2_000_000n,
          balance: 2_000_000n,
          sweepable: 2_000_000n,
          signatures: [SIG],
        }),
      ],
    });
    assert.equal(result.actions.length, 0);
    assert.match(result.notes[0]?.reason ?? "", /different account/);
  });

  it("times out an executing raise instead of launching", () => {
    const result = plan({
      state: "Executing",
      creditedTotal: 10_000n,
      sweptTotal: 10_000n,
      fillFeePaid: 150n,
      executionStartedNs: 1_000n,
      executionTimeoutNs: 1_000n,
      contributors: [
        contributor({ balance: 0n, refundable: 0n, raiseRefundable: 500n }),
      ],
      raiseBalance: 500n,
    });
    assert.equal(result.actions[0]?.type, "try_timeout");
    const refund = result.actions.find((action) => action.type === "request_raise_refund");
    assert.ok(refund && refund.type === "request_raise_refund");
    assert.equal(refund.lamports, 500n);
    assert.equal(refund.destination, refund.contributor);
    assert.equal(
      result.actions.some((action) => action.type === "request_launch"),
      false,
    );
  });
});
