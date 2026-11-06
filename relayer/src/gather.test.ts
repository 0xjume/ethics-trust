import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { planTick } from "./driver.ts";
import { gatherPlan, type ChainReads, type RawTrust } from "./gather.ts";

const SIG = "S".repeat(88);

describe("gatherPlan", () => {
  it("reads the deposit balance and the planner reports the delta", async () => {
    const balances: string[] = [];
    const trust: RawTrust = {
      id: 4,
      state: "Raising",
      watcher: "watcher.testnet",
      target_lamports: 10_000,
      credited_total: 0,
      deadline_ns: "1793760063951000000",
      execution_timeout_ns: "21600000000000",
      filled_at_ns: null,
      execution_started_ns: null,
      fill_fee_bps: 150,
      fill_fee_paid: 0,
      raise_address: null,
      fee_address: null,
      mint_address: null,
      dbc_config: "ConfigPubkey11111111111111111111111111",
      name: "Keyless",
      symbol: "KLT",
      uri: "https://ethics.ltd/k.json",
      curve: {
        sqrt_start_price: "1",
        migration_quote_threshold: "1",
        collect_fee_mode: 0,
        enable_first_swap_with_min_fee: true,
        base_fee_mode: 0,
        cliff_fee_numerator: "1",
        number_of_period: 0,
        reduction_factor: "0",
        points: [],
      },
    };
    const chain: ChainReads = {
      async nowNs() {
        return 1_000n;
      },
      async listTrusts() {
        return [trust];
      },
      async listContributorKeys() {
        return [{ trustId: "4", contributor: "ContributorPubkey111111111111111111111" }];
      },
      async getContribution() {
        return {
          contributor: "ContributorPubkey111111111111111111111",
          deposit_address: "DepositPubkey1111111111111111111111111",
          reported_lamports: 0,
          credited_lamports: 0,
          overflow_lamports: 0,
          swept_lamports: 0,
          refunded_lamports: 0,
          last_solana_sig: "",
        };
      },
      async sweepable() {
        return 0n;
      },
      async refundable() {
        return 0n;
      },
      async raiseRefundable() {
        return 0n;
      },
      async claimable() {
        return 0n;
      },
      async listPending() {
        return [];
      },
      async getBalance(address) {
        balances.push(address);
        return address.startsWith("Deposit") ? 400n : 0n;
      },
      async signaturesFor() {
        return [SIG];
      },
      curveMatches() {
        return true;
      },
    };
    const gathered = await gatherPlan(
      chain,
      { timeoutAttempted: new Set(), burnedSignatures: new Set() },
      "watcher.testnet",
      null,
    );
    assert.equal(gathered.trusts[0]?.deadlineNs, 1793760063951000000n);
    assert.equal(balances.includes("DepositPubkey1111111111111111111111111"), true);
    const plan = planTick(gathered);
    const report = plan.actions.find((action) => action.type === "report_deposit");
    assert.ok(report && report.type === "report_deposit");
    assert.equal(report.lamports, 400n);
    assert.equal(report.solanaSig, SIG);
  });
});
