/**
 * Long-running watcher and relayer.
 *
 * Dry-run (SOLANA_SUBMIT unset) polls, logs the action it would take, and
 * does not send a NEAR or Solana transaction. SOLANA_SUBMIT=1 reports,
 * requests signatures, submits on devnet, and confirms.
 *
 * Missing keys exit immediately. This process does not hang waiting for them.
 */
import { loadConfig } from "./config.ts";
import { executeAction } from "./execute.ts";
import { gatherPlan } from "./gather.ts";
import { startHealthServer } from "./health.ts";
import { errorText, logEvent } from "./log.ts";
import { planTick } from "./driver.ts";
import { createChain, createExecutePorts } from "./runtime.ts";
import { emptyMemory, runLoop, runTick } from "./service.ts";

const config = (() => {
  try {
    return loadConfig(process.env);
  } catch (error) {
    console.error(errorText(error));
    process.exit(1);
  }
})();

logEvent({
  action: "start",
  contractId: config.contractId,
  watcher: config.watcherAccountId,
  relayer: config.relayer.publicKey.toBase58(),
  solanaSubmit: config.solanaSubmit,
  pollMs: config.pollMs,
  trustId: config.trustId,
  healthPort: config.healthPort,
});

if (!config.solanaSubmit) {
  logEvent({
    action: "note",
    reason: "SOLANA_SUBMIT is not 1. Polling in dry-run. No NEAR or Solana transaction will be sent.",
  });
}

const chain = createChain(config);
const ports = createExecutePorts(config, chain);
const memory = emptyMemory();
const signal = new AbortController();
for (const name of ["SIGINT", "SIGTERM"] as const) {
  process.on(name, () => signal.abort());
}

if (config.healthPort != null) {
  try {
    await startHealthServer(config.healthPort);
    logEvent({ action: "health", port: config.healthPort });
  } catch (error) {
    console.error(`Health server failed: ${errorText(error)}`);
    process.exit(1);
  }
}

await runLoop({
  pollMs: config.pollMs,
  signal: signal.signal,
  async cycle() {
    const gathered = await gatherPlan(chain, memory, config.watcherAccountId, config.trustId);
    const plan = planTick(gathered);
    await runTick({
      plan,
      submit: config.solanaSubmit,
      memory,
      execute: (action) => executeAction(action, ports),
    });
  },
});

logEvent({ action: "stop" });
