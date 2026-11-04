import { parseNearJson, ledgerKeysFromState } from "@keyless-trust/ui/format";
import { CONTRACT_ID, NEAR_RPC } from "./config";

export type TrustState =
  | "Raising"
  | "Filled"
  | "Executing"
  | "Launched"
  | "Distributed"
  | "Refunding";

export type CurvePoint = { sqrt_price: string; liquidity: string };

export type DbcCurve = {
  sqrt_start_price: string;
  migration_quote_threshold: string | number;
  collect_fee_mode: number;
  enable_first_swap_with_min_fee: boolean;
  base_fee_mode: number;
  cliff_fee_numerator: string | number;
  number_of_period: number;
  reduction_factor: string | number;
  points: CurvePoint[];
};

export type TrustView = {
  id: number;
  target_lamports: string | number;
  cap_bps: number;
  fill_fee_bps: number;
  deadline_ns: string | number;
  execution_timeout_ns: string | number;
  filled_at_ns: string | number | null;
  execution_started_ns: string | number | null;
  raise_path: string;
  fee_path: string;
  raise_address: string | null;
  fee_address: string | null;
  mint_path: string;
  mint_address: string | null;
  pool_address: string | null;
  fill_fee_paid: string | number;
  dbc_config: string;
  name: string;
  symbol: string;
  uri: string;
  curve: DbcCurve;
  params_hash: string;
  credited_total: string | number;
  overflow_total: string | number;
  tokens_bought: string | number;
  claimable_total: string | number;
  distributed_total: string | number;
  watcher: string;
  state: TrustState;
};

export type Contribution = {
  trust_id: number;
  contributor: string;
  deposit_path: string;
  deposit_address: string | null;
  reported_lamports: string | number;
  credited_lamports: string | number;
  overflow_lamports: string | number;
  swept_lamports: string | number;
  refunded_lamports: string | number;
  raise_refunded_lamports: string | number;
  distributed_tokens: string | number;
  last_solana_sig: string;
};

export type DistributionStatus = {
  trust_id: number;
  state: TrustState;
  tokens_bought: string | number;
  claimable_total: string | number;
  distributed_total: string | number;
  dust_tokens: string | number;
  decimals: number;
  token_program: string;
};

export type PendingTransfer = {
  trust_id: number;
  contributor: string;
  kind: string;
  lamports: string | number;
  message_hash: string;
  expires_at_ns: string | number;
  settled: boolean;
  released: boolean;
  nonce: string | number;
  superseded: boolean;
  extra: string;
};

export type DepositAddressView = {
  trust_id: number;
  contributor: string;
  path: string;
  predecessor: string;
  mpc_contract: string;
  domain_id: string | number;
};

type RpcFailure = { error?: { message?: string; data?: unknown } };

async function rpc<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const response = await fetch(NEAR_RPC, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "keyless-trust", method, params }),
  });
  if (!response.ok) {
    throw new Error(`NEAR RPC ${response.status}`);
  }
  const body = (await response.json()) as RpcFailure & { result?: T };
  if (body.error) {
    const detail = body.error.message ?? "NEAR RPC error";
    throw new Error(detail);
  }
  return body.result as T;
}

export async function viewCall<T>(methodName: string, args: Record<string, unknown> = {}): Promise<T> {
  const argsBase64 = btoa(JSON.stringify(args));
  const result = await rpc<{ result: number[] }>("query", {
    request_type: "call_function",
    finality: "final",
    account_id: CONTRACT_ID,
    method_name: methodName,
    args_base64: argsBase64,
  });
  const text = new TextDecoder().decode(Uint8Array.from(result.result));
  return parseNearJson(text) as T;
}

export function listTrusts(): Promise<TrustView[]> {
  return viewCall<TrustView[]>("list_trusts", { from_index: 0, limit: 50 });
}

export function getTrust(trustId: number): Promise<TrustView | null> {
  return viewCall<TrustView | null>("get_trust", { trust_id: trustId });
}

export function getContribution(trustId: number, contributor: string): Promise<Contribution | null> {
  return viewCall("get_contribution", { trust_id: trustId, contributor });
}

export function claimable(trustId: number, contributor: string): Promise<string | number> {
  return viewCall("claimable", { trust_id: trustId, contributor });
}

export function refundable(trustId: number, contributor: string): Promise<string | number> {
  return viewCall("refundable", { trust_id: trustId, contributor });
}

export function raiseRefundable(trustId: number, contributor: string): Promise<string | number> {
  return viewCall("raise_refundable", { trust_id: trustId, contributor });
}

export function fillFee(trustId: number): Promise<string | number> {
  return viewCall("fill_fee", { trust_id: trustId });
}

export function distributionStatus(trustId: number): Promise<DistributionStatus> {
  return viewCall("distribution_status", { trust_id: trustId });
}

export function getPending(messageHash: string): Promise<PendingTransfer | null> {
  return viewCall("get_pending", { message_hash: messageHash });
}

export function isConsumed(messageHash: string): Promise<boolean> {
  return viewCall("is_consumed", { message_hash: messageHash });
}

export function derivationPaths(trustId: number, solanaPubkey?: string): Promise<string[]> {
  const args: Record<string, unknown> = { trust_id: trustId };
  if (solanaPubkey) args.solana_pubkey = solanaPubkey;
  return viewCall("derivation_paths", args);
}

export function mpcContract(): Promise<string> {
  return viewCall("mpc_contract");
}

export function domainId(): Promise<string | number> {
  return viewCall("domain_id");
}

export function dbcProgram(): Promise<string> {
  return viewCall("dbc_program");
}

export function depositAddress(trustId: number, pubkey: string): Promise<DepositAddressView> {
  return viewCall("deposit_address", { trust_id: trustId, contributor_solana_pubkey: pubkey });
}

export function listAccessKeys(): Promise<{ keys: Array<{ access_key?: { permission?: unknown }; public_key?: string }> }> {
  return rpc("query", {
    request_type: "view_access_key_list",
    finality: "final",
    account_id: CONTRACT_ID,
  });
}

export async function derivedPublicKey(path: string, predecessor: string): Promise<string> {
  const argsBase64 = btoa(JSON.stringify({ path, predecessor, domain_id: 1 }));
  const result = await rpc<{ result: number[] }>("query", {
    request_type: "call_function",
    finality: "final",
    account_id: "v1.signer-prod.testnet",
    method_name: "derived_public_key",
    args_base64: argsBase64,
  });
  const text = new TextDecoder().decode(Uint8Array.from(result.result));
  const parsed = parseNearJson(text);
  return typeof parsed === "string" ? parsed : String(parsed);
}

export function solanaAddressFromNearKey(nearKey: string): string {
  return nearKey.startsWith("ed25519:") ? nearKey.slice("ed25519:".length) : nearKey;
}

/** Contributor pubkeys stored in the contract ledger. No wallet required. */
export async function listContributorKeys(): Promise<Array<{ trustId: string; contributor: string }>> {
  const prefix = btoa("cv");
  const result = await rpc<{ values: Array<{ key: string; value: string }> }>("query", {
    request_type: "view_state",
    finality: "final",
    account_id: CONTRACT_ID,
    prefix_base64: prefix,
  });
  return ledgerKeysFromState(result.values);
}

export function messageHashesFromOutcome(outcome: unknown): string[] {
  const logs: string[] = [];
  const visit = (value: unknown) => {
    if (!value || typeof value !== "object") return;
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    const record = value as Record<string, unknown>;
    if (Array.isArray(record.logs)) {
      for (const line of record.logs) {
        if (typeof line === "string") logs.push(line);
      }
    }
    for (const child of Object.values(record)) visit(child);
  };
  visit(outcome);
  const hashes: string[] = [];
  for (const line of logs) {
    const marker = "EVENT_JSON:";
    const at = line.indexOf(marker);
    if (at < 0) continue;
    try {
      const event = JSON.parse(line.slice(at + marker.length)) as {
        data?: Array<{ message_hash?: string }>;
      };
      const hash = event.data?.[0]?.message_hash;
      if (hash) hashes.push(hash);
    } catch {
      /* log line was not the event we store */
    }
  }
  return hashes;
}
