/**
 * Turn one planned action into NEAR calls and, when the action signs a
 * Solana message, a relayer-paid transaction. Destinations come from the
 * plan, which only copies addresses the contract already stored.
 */
import { createHash } from "node:crypto";
import { Keypair, PublicKey, type Transaction } from "@solana/web3.js";
import { assembleRelayerTransaction } from "./assemble-tx.ts";
import { buildDistributeMessageBytes } from "./build-distribute-message.ts";
import type { PlannedAction } from "./driver.ts";
import { logEvent } from "./log.ts";
import type { NearCallResult } from "./near-outcome.ts";
import { extractEd25519Signature } from "./relay-launch.ts";
import { systemTransferMessage } from "./transfer-message.ts";

export const SIGN_DEPOSIT_YOCTO = 10n ** 22n;
const ONE_YOCTO = 1n;

export type NearCall = {
  method: string;
  args: Record<string, unknown>;
  gasTgas: number;
  depositYocto: bigint;
};

export type Blockhash = { blockhash: string; lastValidBlockHeight: number };

export type ExecutePorts = {
  relayer: Keypair;
  blockhash(): Promise<Blockhash>;
  call(req: NearCall): Promise<NearCallResult>;
  submitSolana(tx: Transaction, ctx: Blockhash): Promise<string>;
  buildLaunch(
    action: Extract<PlannedAction, { type: "request_launch" }>,
  ): Promise<{ message: Buffer } & Blockhash>;
};

export type Executed = {
  action: PlannedAction["type"];
  trustId: string;
  nearTx: string | null;
  solanaSig?: string;
  messageHash?: string;
};

function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function trustId(id: string): number {
  return Number(id);
}

function u64(value: bigint, label: string): number {
  if (value < 0n || value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`${label} does not fit a NEAR json number`);
  }
  return Number(value);
}

function launchSignatures(value: unknown): { raise: Uint8Array; mint: Uint8Array } {
  const obj = value as { raise?: unknown; mint?: unknown } | null;
  if (!obj || typeof obj !== "object" || obj.raise == null || obj.mint == null) {
    throw new Error("launch MPC response missing raise or mint signature");
  }
  return {
    raise: extractEd25519Signature(obj.raise),
    mint: extractEd25519Signature(obj.mint),
  };
}

async function callNear(
  ports: ExecutePorts,
  action: PlannedAction,
  req: NearCall,
): Promise<NearCallResult> {
  const result = await ports.call(req);
  logEvent({
    trustId: action.trustId,
    state: "state" in action ? action.state : undefined,
    action: req.method,
    nearTx: result.txHash,
  });
  return result;
}

async function submitAndConfirm(args: {
  ports: ExecutePorts;
  action: PlannedAction;
  message: Uint8Array;
  messageHash: string;
  vaultSignatures: Array<{ address: string; signature: Uint8Array }>;
  block: Blockhash;
  confirm: "confirm_transfer" | "confirm_launch_outcome";
}): Promise<Executed> {
  const tx = assembleRelayerTransaction({
    message: args.message,
    vaultSignatures: args.vaultSignatures,
    relayer: args.ports.relayer,
  });
  const solanaSig = await args.ports.submitSolana(tx, args.block);
  logEvent({
    trustId: args.action.trustId,
    state: args.action.state,
    action: "solana_submit",
    solanaSig,
    messageHash: args.messageHash,
  });
  const confirmed = await callNear(args.ports, args.action, {
    method: args.confirm,
    args: { message_hash: args.messageHash, solana_sig: solanaSig },
    gasTgas: 30,
    depositYocto: ONE_YOCTO,
  });
  return {
    action: args.action.type,
    trustId: args.action.trustId,
    nearTx: confirmed.txHash,
    solanaSig,
    messageHash: args.messageHash,
  };
}

export async function executeAction(
  action: PlannedAction,
  ports: ExecutePorts,
): Promise<Executed> {
  return executeInner(action, ports);
}

async function executeInner(action: PlannedAction, ports: ExecutePorts): Promise<Executed> {
  if (action.type === "try_timeout") {
    const result = await callNear(ports, action, {
      method: "try_timeout",
      args: { trust_id: trustId(action.trustId) },
      gasTgas: 30,
      depositYocto: 0n,
    });
    return { action: action.type, trustId: action.trustId, nearTx: result.txHash };
  }
  if (action.type === "release_pending") {
    const result = await callNear(ports, action, {
      method: "release_pending",
      args: { message_hash: action.messageHash },
      gasTgas: 30,
      depositYocto: 0n,
    });
    return { action: action.type, trustId: action.trustId, nearTx: result.txHash };
  }
  if (action.type === "report_deposit") {
    const result = await callNear(ports, action, {
      method: "report_deposit",
      args: {
        trust_id: trustId(action.trustId),
        contributor: action.contributor,
        lamports: u64(action.lamports, "lamports"),
        solana_sig: action.solanaSig,
      },
      gasTgas: 30,
      depositYocto: ONE_YOCTO,
    });
    return { action: action.type, trustId: action.trustId, nearTx: result.txHash };
  }
  if (
    action.type === "request_sweep" ||
    action.type === "request_refund" ||
    action.type === "request_raise_refund" ||
    action.type === "request_fill_fee"
  ) {
    const block = await ports.blockhash();
    const message = systemTransferMessage({
      relayer: ports.relayer.publicKey,
      source: new PublicKey(action.source),
      destination: new PublicKey(action.destination),
      lamports: action.lamports,
      blockhash: block.blockhash,
    });
    const messageHash = sha256Hex(message);
    const args =
      action.type === "request_fill_fee"
        ? { trust_id: trustId(action.trustId), message: message.toString("base64") }
        : {
            trust_id: trustId(action.trustId),
            contributor: action.contributor,
            message: message.toString("base64"),
          };
    const signed = await callNear(ports, action, {
      method: action.type,
      args,
      gasTgas: 80,
      depositYocto: SIGN_DEPOSIT_YOCTO,
    });
    return submitAndConfirm({
      ports,
      action,
      message,
      messageHash,
      vaultSignatures: [
        { address: action.source, signature: extractEd25519Signature(signed.value) },
      ],
      block,
      confirm: "confirm_transfer",
    });
  }
  if (action.type === "request_launch") {
    const built = await ports.buildLaunch(action);
    const messageHash = sha256Hex(built.message);
    const signed = await callNear(ports, action, {
      method: "request_launch",
      args: {
        trust_id: trustId(action.trustId),
        message: built.message.toString("base64"),
      },
      gasTgas: 90,
      depositYocto: SIGN_DEPOSIT_YOCTO,
    });
    const sigs = launchSignatures(signed.value);
    return submitAndConfirm({
      ports,
      action,
      message: built.message,
      messageHash,
      vaultSignatures: [
        { address: action.raise, signature: sigs.raise },
        { address: action.mint, signature: sigs.mint },
      ],
      block: built,
      confirm: "confirm_launch_outcome",
    });
  }
  const block = await ports.blockhash();
  const message = buildDistributeMessageBytes({
    relayer: ports.relayer.publicKey,
    raise: new PublicKey(action.raise),
    mint: new PublicKey(action.mint),
    legs: action.legs.map((leg) => ({
      owner: new PublicKey(leg.owner),
      amount: leg.amount,
      createAta: true,
    })),
    blockhash: block.blockhash,
  });
  const messageHash = sha256Hex(message);
  const signed = await callNear(ports, action, {
    method: "request_distribute",
    args: { trust_id: trustId(action.trustId), message: message.toString("base64") },
    gasTgas: 60,
    depositYocto: SIGN_DEPOSIT_YOCTO,
  });
  return submitAndConfirm({
    ports,
    action,
    message,
    messageHash,
    vaultSignatures: [
      { address: action.raise, signature: extractEd25519Signature(signed.value) },
    ],
    block,
    confirm: "confirm_transfer",
  });
}
