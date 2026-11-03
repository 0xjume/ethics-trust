import { JsonRpcProvider } from "near-api-js";

export const TESTNET_RPC =
  process.env.NEAR_RPC_URL ?? "https://rpc.testnet.near.org";
export const MPC_CONTRACT =
  process.env.MPC_CONTRACT ?? "v1.signer-prod.testnet";

export function nearProvider(): JsonRpcProvider {
  return new JsonRpcProvider({ url: TESTNET_RPC });
}

export async function viewDerivedPublicKey(args: {
  path: string;
  predecessor: string;
  domain_id?: number;
}): Promise<string> {
  const provider = nearProvider();
  const result = await provider.callFunction({
    contractId: MPC_CONTRACT,
    method: "derived_public_key",
    args: {
      path: args.path,
      predecessor: args.predecessor,
      domain_id: args.domain_id ?? 1,
    },
  });
  if (typeof result === "string") {
    return result;
  }
  return String(result);
}

export function solanaAddressFromNearKey(nearKey: string): string {
  return nearKey.startsWith("ed25519:") ? nearKey.slice("ed25519:".length) : nearKey;
}
