/**
 * Deploy `keyless-trust` to a throwaway subaccount of
 * `keyless-trust-dev-54ef71.testnet` (or $NEAR_ACCOUNT_ID).
 *
 * Testnet only. Refuses mainnet. Does nothing unless EXECUTE_DEPLOY=1
 * and NEAR_ACCOUNT_ID + NEAR_PRIVATE_KEY are set. Never prints the private key.
 *
 *   cargo build -p keyless-trust --target wasm32-unknown-unknown --release
 *   NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
 *   NEAR_PRIVATE_KEY=ed25519:... \
 *   EXECUTE_DEPLOY=1 \
 *   pnpm --filter @keyless-trust/relayer deploy-testnet
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  Account,
  JsonRpcProvider,
  KeyPair,
  type KeyPairString,
  actions,
  nearToYocto,
  teraToGas,
} from "near-api-js";
import { TESTNET_RPC } from "./near-rpc.ts";

const PARENT_DEFAULT = "keyless-trust-dev-54ef71.testnet";

export function resolveDeployTarget(env: Record<string, string | undefined> = process.env) {
  const network = env.NEAR_NETWORK ?? "testnet";
  if (network !== "testnet") {
    throw new Error("refusing to deploy: NEAR_NETWORK must be testnet");
  }
  const parent = env.NEAR_ACCOUNT_ID ?? PARENT_DEFAULT;
  if (parent.endsWith(".near") && !parent.endsWith(".testnet")) {
    throw new Error("refusing to deploy: parent looks like mainnet");
  }
  const sub = env.DEPLOY_SUBACCOUNT ?? "m2";
  if (!/^[a-z0-9_-]+$/.test(sub) || sub.includes(".")) {
    throw new Error("DEPLOY_SUBACCOUNT must be a single lowercase label");
  }
  return {
    network,
    parent,
    contractId: `${sub}.${parent}`,
    wasmPath:
      env.WASM_PATH ??
      resolve(process.cwd(), "target/wasm32-unknown-unknown/release/keyless_trust.wasm"),
  };
}

export async function deployTestnet(env: Record<string, string | undefined> = process.env) {
  if (env.EXECUTE_DEPLOY !== "1") {
    return { skipped: true as const, reason: "EXECUTE_DEPLOY is not 1" };
  }
  const accountId = env.NEAR_ACCOUNT_ID;
  const privateKey = env.NEAR_PRIVATE_KEY as KeyPairString | undefined;
  if (!accountId || !privateKey) {
    throw new Error("NEAR_ACCOUNT_ID and NEAR_PRIVATE_KEY are required to deploy");
  }

  const target = resolveDeployTarget(env);
  const wasm = new Uint8Array(readFileSync(target.wasmPath));
  const provider = new JsonRpcProvider({ url: env.NEAR_RPC_URL ?? TESTNET_RPC });
  const parent = new Account(accountId, provider, privateKey);
  const publicKey = KeyPair.fromString(privateKey).getPublicKey();

  const outcome = await parent.signAndSendTransaction({
    receiverId: target.contractId,
    actions: [
      actions.createAccount(),
      actions.transfer(nearToYocto("4")),
      actions.addFullAccessKey(publicKey),
      actions.deployContract(wasm),
      actions.functionCall(
        "new",
        {
          mpc_contract: env.MPC_CONTRACT ?? null,
          dbc_program: null,
        },
        teraToGas(30),
        0n,
      ),
    ],
  });

  const hash = outcome.transaction.hash;
  return {
    skipped: false as const,
    contractId: target.contractId,
    transactionHash: hash,
    explorer: `https://testnet.nearblocks.io/txns/${hash}`,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const result = await deployTestnet();
  console.log(JSON.stringify(result, null, 2));
}
