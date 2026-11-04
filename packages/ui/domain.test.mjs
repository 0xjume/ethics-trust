import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  countContributors,
  formatNearTime,
  formatSol,
  formatTokens,
  ledgerKeysFromState,
  parseNearJson,
  proRataShare,
  sharePercent,
  solanaExplorerAddress,
  statusLabel,
} from "./format.mjs";
import {
  DEMO_CURVE,
  buildCreateTrustCli,
  buildDistributeCli,
  isSolanaPubkey,
  stringifyNearArgs,
  validateCreateTrust,
} from "./validate.mjs";

describe("amounts and status", () => {
  it("formats lamports and token units without float drift", () => {
    assert.equal(formatSol(50_000_000), "0.05");
    assert.equal(formatSol("1000000000"), "1");
    assert.equal(formatTokens("15377469652628"), "15377469.652628");
  });

  it("keeps NEAR nanosecond timestamps that do not fit in a JS number", () => {
    const parsed = parseNearJson('{"deadline_ns":1793760063951000000,"credited_total":50000000}');
    assert.equal(parsed.deadline_ns, "1793760063951000000");
    assert.equal(parsed.credited_total, 50000000);
    assert.match(formatNearTime(parsed.deadline_ns), /UTC$/);
  });

  it("computes share percent and the contract floor share", () => {
    assert.equal(sharePercent(25, 100), "25.00");
    assert.equal(sharePercent(1, 3), "33.33");
    assert.equal(proRataShare(10n, 1n, 3n), 3n);
    assert.equal(statusLabel("Distributed"), "Distributed");
    assert.equal(statusLabel("nope"), "Unknown");
  });

  it("reads contributor keys out of the cv state prefix", () => {
    const value = Buffer.from(
      "\u0000\u0000\u0000\u0000".slice(0, 0) ||
        Uint8Array.from([0x2e, 0, 0, 0, ...Buffer.from("1:CheLEbm6ELdLHSqju5K5srLGBWeFUMStLWFbDhd3nbbn")]),
    ).toString("base64");
    const rows = ledgerKeysFromState([{ value }]);
    assert.equal(rows.length, 1);
    assert.equal(rows[0].trustId, "1");
    assert.equal(countContributors(rows, 1), 1);
    assert.equal(countContributors(rows, 2), 0);
  });

  it("links Solana addresses to devnet", () => {
    assert.match(solanaExplorerAddress("Efmoa3eEh8LqkVgaxQUwN78aKQUF661mb4uLAfXtgCEK"), /cluster=devnet/);
  });
});

describe("create_trust validation", () => {
  const good = {
    target_lamports: "50000000",
    deadline_ns: "1793760063951000000",
    dbc_config: "3TPLwDd588cqc28ntKQYWc9gRs7UY7iL7y1FXcLwQ8mj",
    name: "Keyless",
    symbol: "KLT",
    uri: "https://ethics.ltd/k.json",
    cap_bps: 100,
    fill_fee_bps: 150,
    execution_timeout_ns: "21600000000000",
    watcher: "keyless-trust-dev-54ef71.testnet",
    curve: DEMO_CURVE,
  };

  it("accepts the pinned demo curve and required cap", () => {
    const result = validateCreateTrust(good);
    assert.equal(result.ok, true);
    if (!result.ok) return;
    assert.equal(result.args.cap_bps, 100);
    assert.equal(result.args.curve.points.length, 1);
    const json = stringifyNearArgs(result.args);
    assert.match(json, /1793760063951000000/);
    assert.doesNotMatch(json, /__bigint/);
    const cli = buildCreateTrustCli({
      contractId: "m10.keyless-trust-dev-54ef71.testnet",
      args: result.args,
      accountId: "alice.testnet",
    });
    assert.match(cli, /create_trust/);
    assert.match(cli, /--deposit 0\.01/);
    assert.match(cli, /--networkId testnet/);
  });

  it("rejects a missing cap, a long uri, and a bad config pubkey", () => {
    const result = validateCreateTrust({
      ...good,
      cap_bps: "",
      uri: "x".repeat(61),
      dbc_config: "not-a-key",
      curve: { ...DEMO_CURVE, base_fee_mode: 1, points: [] },
    });
    assert.equal(result.ok, false);
    if (result.ok) return;
    assert.match(result.errors.join(" "), /cap_bps/);
    assert.match(result.errors.join(" "), /60 characters/);
    assert.match(result.errors.join(" "), /DBC config/);
    assert.match(result.errors.join(" "), /linear fee/);
  });

  it("accepts the live devnet pubkeys used by m10", () => {
    assert.equal(isSolanaPubkey("3TPLwDd588cqc28ntKQYWc9gRs7UY7iL7y1FXcLwQ8mj"), true);
    assert.equal(isSolanaPubkey("short"), false);
  });

  it("prints the distribute call with 0.01 NEAR and 60 Tgas", () => {
    const cli = buildDistributeCli({
      contractId: "m10.keyless-trust-dev-54ef71.testnet",
      trustId: 1,
      messageBase64: "AQID",
      accountId: "alice.testnet",
    });
    assert.match(cli, /request_distribute/);
    assert.match(cli, /--deposit 0\.01/);
    assert.match(cli, /60 Tgas/);
    assert.match(cli, /AQID/);
  });
});
