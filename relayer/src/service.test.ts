import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { emptyMemory, runTick } from "./service.ts";
import type { PlanResult } from "./driver.ts";

describe("runTick", () => {
  it("dry-run logs the plan and does not execute it", async () => {
    let called = 0;
    const plan: PlanResult = {
      actions: [
        {
          type: "report_deposit",
          trustId: "1",
          state: "Raising",
          contributor: "ContributorPubkey111111111111111111111",
          lamports: 10n,
          solanaSig: "S".repeat(88),
        },
      ],
      notes: [],
    };
    const result = await runTick({
      plan,
      submit: false,
      memory: emptyMemory(),
      async execute() {
        called += 1;
        throw new Error("dry-run must not execute");
      },
    });
    assert.equal(called, 0);
    assert.equal(result.executed.length, 0);
  });

  it("records a successful timeout so the next tick can skip it", async () => {
    const memory = emptyMemory();
    await runTick({
      plan: {
        actions: [{ type: "try_timeout", trustId: "9", state: "Refunding" }],
        notes: [],
      },
      submit: true,
      memory,
      async execute() {
        return { action: "try_timeout", trustId: "9", nearTx: "abc" };
      },
    });
    assert.equal(memory.timeoutAttempted.has("9"), true);
  });
});
