/**
 * Live NEAR testnet and Solana devnet adapters. Views go through JSON so
 * u64 timestamps stay exact. Change calls use the watcher key.
 */
import { Account, JsonRpcProvider, teraToGas, type KeyPairString } from "near-api-js";
import { Connection, PublicKey } from "@solana/web3.js";
import { buildLaunchMessageForTrust } from "./build-launch-message.ts";
import type { RelayerConfig } from "./config.ts";
import { curveMatchesBuilder } from "./curve.ts";
import { decodePending, ledgerKeysFromState } from "./ledger.ts";
import { asBigInt, parseNearJson } from "./near-json.ts";
import { nearCallResult } from "./near-outcome.ts";
import type { ChainReads, RawContribution, RawTrust } from "./gather.ts";
import type { ExecutePorts, NearCall } from "./execute.ts";
import type { NearCallResult } from "./near-outcome.ts";
import type { PlanInput } from "./driver.ts";

export type LiveChain = ChainReads & {
  call(req: NearCall): Promise<NearCallResult>;
  connection: Connection;
};

const LEDGER_PREFIX = Buffer.from("cv").toString("base64");
const PENDING_PREFIX = Buffer.from("p").toString("base64");

type RpcError = { error?: { message?: string; data?: unknown } };

async function rpc<T>(url: string, method: string, params: unknown): Promise<T> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "keyless-trust-relayer", method, params }),
  });
  if (!response.ok) throw new Error(`NEAR RPC ${response.status}`);
  const body = (await response.json()) as RpcError & { result?: T };
  if (body.error) throw new Error(body.error.message ?? "NEAR RPC error");
  return body.result as T;
}

export function createChain(config: RelayerConfig): LiveChain {
  const provider = new JsonRpcProvider({ url: config.nearRpcUrl });
  const account = new Account(
    config.watcherAccountId,
    provider,
    config.watcherPrivateKey as KeyPairString,
  );
  const connection = new Connection(config.solanaRpcUrl, "confirmed");

  async function view<T>(method: string, args: Record<string, unknown>): Promise<T> {
    const result = await rpc<{ result: number[] }>(config.nearRpcUrl, "query", {
      request_type: "call_function",
      finality: "final",
      account_id: config.contractId,
      method_name: method,
      args_base64: Buffer.from(JSON.stringify(args)).toString("base64"),
    });
    const text = Buffer.from(result.result).toString("utf8");
    return parseNearJson(text) as T;
  }

  async function viewState(prefix: string): Promise<Array<{ key: string; value: string }>> {
    const values: Array<{ key: string; value: string }> = [];
    let after: string | undefined;
    for (let pageNo = 0; pageNo < 20; pageNo += 1) {
      const page = await rpc<{
        values: Array<{ key: string; value: string }>;
        last_key?: string | null;
      }>(config.nearRpcUrl, "query", {
        request_type: "view_state",
        finality: "final",
        account_id: config.contractId,
        prefix_base64: prefix,
        ...(after ? { after_key_base64: after } : {}),
      });
      values.push(...(page.values ?? []));
      if (!page.last_key || page.values.length === 0 || page.last_key === after) break;
      after = page.last_key;
    }
    return values;
  }

  const chain: LiveChain = {
    async nowNs() {
      const block = await rpc<{ header: { timestamp_nanosec: string } }>(
        config.nearRpcUrl,
        "block",
        { finality: "final" },
      );
      return asBigInt(block.header.timestamp_nanosec);
    },
    async listTrusts() {
      const all: RawTrust[] = [];
      let from = 0;
      for (;;) {
        const page = await view<RawTrust[]>("list_trusts", { from_index: from, limit: 50 });
        if (!Array.isArray(page) || page.length === 0) break;
        all.push(...page);
        if (page.length < 50) break;
        from += page.length;
      }
      return all;
    },
    async listContributorKeys() {
      const values = await viewState(LEDGER_PREFIX);
      return ledgerKeysFromState(values);
    },
    async getContribution(trustId, contributor) {
      return view<RawContribution | null>("get_contribution", {
        trust_id: Number(trustId),
        contributor,
      });
    },
    async sweepable(trustId, contributor) {
      return asBigInt(await view("sweepable", { trust_id: Number(trustId), contributor }));
    },
    async refundable(trustId, contributor) {
      return asBigInt(await view("refundable", { trust_id: Number(trustId), contributor }));
    },
    async raiseRefundable(trustId, contributor) {
      return asBigInt(
        await view("raise_refundable", { trust_id: Number(trustId), contributor }),
      );
    },
    async claimable(trustId, contributor) {
      return asBigInt(await view("claimable", { trust_id: Number(trustId), contributor }));
    },
    async listPending() {
      const values = await viewState(PENDING_PREFIX);
      const rows: PlanInput["pendings"] = [];
      for (const entry of values) {
        const decoded = decodePending(Buffer.from(entry.value, "base64"));
        if (decoded) rows.push(decoded);
      }
      return rows;
    },
    async getBalance(address) {
      try {
        const lamports = await connection.getBalance(new PublicKey(address), "confirmed");
        return BigInt(lamports);
      } catch {
        return 0n;
      }
    },
    async signaturesFor(address) {
      const rows = await connection.getSignaturesForAddress(new PublicKey(address), { limit: 20 });
      return rows.map((row) => row.signature);
    },
    curveMatches: curveMatchesBuilder,
    async call(req) {
      const outcome = await account.callFunctionRaw({
        contractId: config.contractId,
        methodName: req.method,
        args: req.args,
        gas: teraToGas(req.gasTgas),
        deposit: req.depositYocto,
        waitUntil: "FINAL",
      });
      return nearCallResult(outcome);
    },
    connection,
  };
  return chain;
}

export function createExecutePorts(config: RelayerConfig, chain: LiveChain): ExecutePorts {
  return {
    relayer: config.relayer,
    async blockhash() {
      const latest = await chain.connection.getLatestBlockhash("confirmed");
      return {
        blockhash: latest.blockhash,
        lastValidBlockHeight: latest.lastValidBlockHeight,
      };
    },
    call: (req) => chain.call(req),
    async submitSolana(tx, ctx) {
      const signature = await chain.connection.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
      });
      await chain.connection.confirmTransaction(
        {
          signature,
          blockhash: ctx.blockhash,
          lastValidBlockHeight: ctx.lastValidBlockHeight,
        },
        "confirmed",
      );
      return signature;
    },
    async buildLaunch(action) {
      const latest = await chain.connection.getLatestBlockhash("confirmed");
      const built = await buildLaunchMessageForTrust({
        connection: chain.connection,
        relayer: config.relayer.publicKey,
        raise: new PublicKey(action.raise),
        feeVault: new PublicKey(action.fee),
        mint: new PublicKey(action.mint),
        config: new PublicKey(action.dbcConfig),
        name: action.name,
        symbol: action.symbol,
        uri: action.uri,
        buyLamports: action.buyLamports,
        fillFeeLamports: 0n,
        blockhash: latest.blockhash,
      });
      return {
        message: built.message,
        blockhash: latest.blockhash,
        lastValidBlockHeight: latest.lastValidBlockHeight,
      };
    },
  };
}
