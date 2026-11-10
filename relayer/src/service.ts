import { planTick, type PlanResult, type PlannedAction } from "./driver.ts";
import type { Executed } from "./execute.ts";
import { errorText, logEvent } from "./log.ts";

export type RelayerMemory = {
  timeoutAttempted: Set<string>;
  burnedSignatures: Set<string>;
};

export function emptyMemory(): RelayerMemory {
  return { timeoutAttempted: new Set(), burnedSignatures: new Set() };
}

export async function runTick(args: {
  plan: PlanResult;
  submit: boolean;
  memory: RelayerMemory;
  execute(action: PlannedAction): Promise<Executed>;
}): Promise<{ executed: Executed[] }> {
  for (const note of args.plan.notes) {
    logEvent({
      trustId: note.trustId,
      state: note.state,
      action: "note",
      reason: note.reason,
      dryRun: !args.submit,
    });
  }
  if (!args.submit) {
    for (const action of args.plan.actions) {
      logEvent({
        trustId: action.trustId,
        state: action.state,
        action: action.type,
        dryRun: true,
        contributor: "contributor" in action ? action.contributor : undefined,
        lamports: "lamports" in action ? action.lamports : undefined,
        messageHash: action.type === "release_pending" ? action.messageHash : undefined,
      });
    }
    if (args.plan.actions.length === 0 && args.plan.notes.length === 0) {
      logEvent({ action: "idle", dryRun: true });
    }
    return { executed: [] };
  }

  const executed: Executed[] = [];
  for (const action of args.plan.actions) {
    try {
      const result = await args.execute(action);
      executed.push(result);
      if (action.type === "try_timeout") args.memory.timeoutAttempted.add(action.trustId);
      if (action.type === "report_deposit") args.memory.burnedSignatures.add(action.solanaSig);
    } catch (error) {
      const message = errorText(error);
      if (action.type === "report_deposit" && message.includes("already reported")) {
        args.memory.burnedSignatures.add(action.solanaSig);
      }
      logEvent({
        trustId: action.trustId,
        state: action.state,
        action: "action_failed",
        failed: action.type,
        error: message,
      });
    }
  }
  return { executed };
}

export async function runLoop(args: {
  pollMs: number;
  signal: AbortSignal;
  cycle(): Promise<void>;
}): Promise<void> {
  while (!args.signal.aborted) {
    try {
      await args.cycle();
    } catch (error) {
      logEvent({ action: "tick_error", error: errorText(error) });
    }
    if (args.signal.aborted) break;
    await sleep(args.pollMs, args.signal);
  }
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}

export { planTick };
