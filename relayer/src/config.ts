import type { Keypair } from "@solana/web3.js";
import { loadSolanaRelayerKeypair, loadWatcherPrivateKey } from "./keypair.ts";

export const DEFAULT_CONTRACT_ID = "m10.keyless-trust-dev-54ef71.testnet";
export const DEFAULT_NEAR_RPC = "https://rpc.testnet.near.org";
export const DEFAULT_SOLANA_RPC = "https://api.devnet.solana.com";
export const DEFAULT_POLL_MS = 12_000;

export type RelayerConfig = {
  contractId: string;
  nearRpcUrl: string;
  watcherAccountId: string;
  watcherPrivateKey: string;
  solanaRpcUrl: string;
  relayer: Keypair;
  pollMs: number;
  solanaSubmit: boolean;
  trustId: string | null;
  healthPort: number | null;
};

function requireTestnet(name: string, url: string): void {
  if (url.toLowerCase().includes("mainnet")) {
    throw new Error(`${name} must be NEAR testnet or Solana devnet. Refusing ${url}.`);
  }
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): RelayerConfig {
  const missing: string[] = [];
  const watcherAccountId = env.NEAR_WATCHER_ACCOUNT_ID?.trim() ?? "";
  if (!watcherAccountId) missing.push("NEAR_WATCHER_ACCOUNT_ID");
  const hasNearKey = Boolean(
    env.NEAR_WATCHER_PRIVATE_KEY?.trim() || env.NEAR_WATCHER_KEY_PATH?.trim(),
  );
  if (!hasNearKey) missing.push("NEAR_WATCHER_PRIVATE_KEY or NEAR_WATCHER_KEY_PATH");
  if (!env.SOLANA_RELAYER_KEYPAIR?.trim() && !env.SOLANA_KEYPAIR_PATH?.trim()) {
    missing.push("SOLANA_RELAYER_KEYPAIR");
  }
  if (missing.length > 0) {
    throw new Error(
      `The relayer needs ${missing.join(", ")} before it can start. ` +
        "It exits instead of polling without keys. See relayer/README.md.",
    );
  }

  const contractId = (
    env.NEAR_CONTRACT_ID?.trim() ||
    env.CONTRACT_ID?.trim() ||
    DEFAULT_CONTRACT_ID
  );
  if (!contractId.endsWith(".testnet")) {
    throw new Error(`NEAR_CONTRACT_ID must be a testnet account. Refusing ${contractId}.`);
  }
  const nearRpcUrl = env.NEAR_RPC_URL?.trim() || DEFAULT_NEAR_RPC;
  const solanaRpcUrl = env.SOLANA_RPC_URL?.trim() || DEFAULT_SOLANA_RPC;
  requireTestnet("NEAR_RPC_URL", nearRpcUrl);
  requireTestnet("SOLANA_RPC_URL", solanaRpcUrl);

  const pollRaw = env.POLL_MS?.trim();
  const pollMs = pollRaw ? Number(pollRaw) : DEFAULT_POLL_MS;
  if (!Number.isFinite(pollMs) || pollMs < 1_000) {
    throw new Error("POLL_MS must be a number of milliseconds, at least 1000.");
  }

  let healthPort: number | null = null;
  if (env.HEALTH_DISABLE !== "1") {
    const portRaw = env.PORT?.trim() || env.HEALTH_PORT?.trim() || "8080";
    const port = Number(portRaw);
    if (!Number.isInteger(port) || port < 0 || port > 65535) {
      throw new Error("PORT or HEALTH_PORT must be a TCP port.");
    }
    healthPort = port;
  }

  const trustId = env.TRUST_ID?.trim() || null;
  if (trustId && !/^\d+$/.test(trustId)) {
    throw new Error("TRUST_ID must be a numeric trust id.");
  }

  return {
    contractId,
    nearRpcUrl,
    watcherAccountId,
    watcherPrivateKey: loadWatcherPrivateKey(env),
    solanaRpcUrl,
    relayer: loadSolanaRelayerKeypair(env),
    pollMs,
    solanaSubmit: env.SOLANA_SUBMIT === "1",
    trustId,
    healthPort,
  };
}
