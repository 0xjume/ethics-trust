import { parseNearJson } from "./near-json.ts";

export type NearCallResult = {
  txHash: string | null;
  value: unknown;
};

/** Pull the transaction hash and the final receipt value out of an RPC outcome. */
export function nearCallResult(outcome: unknown): NearCallResult {
  if (!outcome || typeof outcome !== "object") {
    return { txHash: null, value: outcome ?? null };
  }
  const record = outcome as {
    transaction_outcome?: { id?: string };
    status?: { SuccessValue?: string; Failure?: unknown } | string;
  };
  const txHash = record.transaction_outcome?.id ?? null;
  const status = record.status;
  if (!status || typeof status === "string") {
    return { txHash, value: outcome };
  }
  if ("Failure" in status && status.Failure !== undefined) {
    throw new Error(`NEAR call failed: ${JSON.stringify(status.Failure)}`);
  }
  if (typeof status.SuccessValue === "string") {
    if (status.SuccessValue.length === 0) return { txHash, value: null };
    const text = Buffer.from(status.SuccessValue, "base64").toString("utf8");
    if (!text) return { txHash, value: null };
    try {
      return { txHash, value: parseNearJson(text) };
    } catch {
      return { txHash, value: text };
    }
  }
  return { txHash, value: outcome };
}
