import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  DAMM_V2_PROGRAM,
  DBC_PROGRAM,
  EXPECTED_ACCOUNTS,
  EXPECTED_IX,
  anchorDisc,
  localSdkDiscriminatorReport,
} from "./verify-dbc-sdk.ts";

describe("DBC SDK 2.0.0 pin vs embedded IDL", () => {
  const report = localSdkDiscriminatorReport();

  it("pins the newest npm releases", () => {
    assert.equal(report.dbcVersion, "2.0.0");
    assert.equal(report.cpVersion, "1.5.1");
    assert.equal(report.cpAmmProgramId, DAMM_V2_PROGRAM);
    assert.equal(report.idlAddress, DBC_PROGRAM);
  });

  it("embeds create_config2 and initialize_virtual_pool_with_spl_token", () => {
    assert.equal(report.hasCreateConfig2, true);
    assert.equal(report.hasInitializeVirtualPool, true);
  });

  it("matches PLAN.md / program 0.2.2 instruction discriminators", () => {
    for (const [name, expected] of Object.entries(EXPECTED_IX)) {
      assert.deepEqual(report.instructions[name].sdk, expected, name);
      assert.equal(report.instructions[name].ok, true, name);
      assert.deepEqual(report.instructions[name].sdk, anchorDisc("global", name), name);
    }
  });

  it("matches PoolConfig and VirtualPool account discriminators", () => {
    for (const [name, expected] of Object.entries(EXPECTED_ACCOUNTS)) {
      assert.deepEqual(report.accounts[name].sdk, expected, name);
      assert.equal(report.accounts[name].ok, true, name);
    }
  });
});
