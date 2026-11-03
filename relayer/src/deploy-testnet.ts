/**
 * Deploy `keyless-trust` to a throwaway subaccount of
 * `keyless-trust-dev-54ef71.testnet` (or $NEAR_ACCOUNT_ID).
 *
 * Testnet only. Refuses mainnet. Does nothing unless EXECUTE_DEPLOY=1
 * and NEAR_ACCOUNT_ID + NEAR_PRIVATE_KEY are set. Never prints the private key.
 *
 * Storage deposit is computed from wasm size (≈1 NEAR / 100 KiB + 0.5 NEAR
 * overhead), with a 5.5 NEAR floor. Override with DEPLOY_NEAR.
 * By default the contract account has **no** full-access key (keyless).
 * Set DEPLOY_KEEP_KEY=1 only if you need the parent key on the contract.
 *
 *   scripts/build-wasm.sh
 *   NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
 *   NEAR_PRIVATE_KEY=ed25519:... \
 *   EXECUTE_DEPLOY=1 \
 *   pnpm --filter @keyless-trust/relayer deploy-testnet
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  Account,
  JsonRpcProvider,
  KeyPair,
  type KeyPairString,
  PublicKey,
  actions,
  nearToYocto,
  teraToGas,
} from "near-api-js";
import { TESTNET_RPC } from "./near-rpc.ts";

const PARENT_DEFAULT = "keyless-trust-dev-54ef71.testnet";
const MIN_DEPLOY_NEAR = 5.5;
const BYTES_PER_NEAR = 100 * 1024;
const ACCOUNT_OVERHEAD_NEAR = 0.5;

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** ≈1 NEAR per 100 KiB of contract code, plus account overhead, floor 5.5. */
export function storageDepositNear(wasmBytes: number): number {
  if (wasmBytes <= 0) {
    return MIN_DEPLOY_NEAR;
  }
  const computed = wasmBytes / BYTES_PER_NEAR + ACCOUNT_OVERHEAD_NEAR;
  return Math.max(MIN_DEPLOY_NEAR, Math.ceil(computed * 10) / 10);
}

export function defaultWasmPath(): string {
  return resolve(REPO_ROOT, "target/wasm32-unknown-unknown/release/keyless_trust.wasm");
}

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
    wasmPath: env.WASM_PATH ?? defaultWasmPath(),
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
  const depositNear = env.DEPLOY_NEAR
    ? Number(env.DEPLOY_NEAR)
    : storageDepositNear(wasm.byteLength);
  if (!Number.isFinite(depositNear) || depositNear < MIN_DEPLOY_NEAR) {
    throw new Error(`DEPLOY_NEAR must be >= ${MIN_DEPLOY_NEAR}`);
  }
  const provider = new JsonRpcProvider({ url: env.NEAR_RPC_URL ?? TESTNET_RPC });
  const parent = new Account(accountId, provider, privateKey);
  const publicKey = KeyPair.fromString(privateKey).getPublicKey();
  const keepKey = keepDeployFullAccessKey(env);

  const outcome = await parent.signAndSendTransaction({
    receiverId: target.contractId,
    actions: deployAccountActions({
      wasm,
      depositNear,
      publicKey,
      keepFullAccessKey: keepKey,
      mpcContract: env.MPC_CONTRACT ?? null,
    }),
  });

  const hash = outcome.transaction.hash;
  return {
    skipped: false as const,
    contractId: target.contractId,
    transactionHash: hash,
    explorer: `https://testnet.nearblocks.io/txns/${hash}`,
    depositNear,
    wasmBytes: wasm.byteLength,
    keepFullAccessKey: keepKey,
  };
}

export function keepDeployFullAccessKey(
  env: Record<string, string | undefined> = process.env,
): boolean {
  return env.DEPLOY_KEEP_KEY === "1";
}

export function deployAccountActions(args: {
  wasm: Uint8Array;
  depositNear: number;
  publicKey: PublicKey;
  keepFullAccessKey: boolean;
  mpcContract: string | null;
}) {
  const out = [
    actions.createAccount(),
    actions.transfer(nearToYocto(args.depositNear)),
  ];
  if (args.keepFullAccessKey) {
    out.push(actions.addFullAccessKey(args.publicKey));
  }
  out.push(
    actions.deployContract(args.wasm),
    actions.functionCall(
      "new",
      {
        mpc_contract: args.mpcContract,
        dbc_program: null,
      },
      teraToGas(30),
      0n,
    ),
  );
  return out;
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const result = await deployTestnet();
  console.log(JSON.stringify(result, null, 2));
}
