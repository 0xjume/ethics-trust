/**
 * Turn `deposit_address(trust_id, contributor)` into a Solana address.
 *
 * The contract view returns path `trust/<id>/in/<pubkey>` plus the MPC inputs.
 * This script calls `v1.signer-prod.testnet.derived_public_key` (domain 1).
 * After `create_trust` / `register` the contract stores the same key via
 * an MPC callback — wait until `get_trust.raise_address` and
 * `get_contribution.deposit_address` are set before sweep/refund.
 * Read-only. No keys, no spend.
 *
 * Env:
 *   TRUST_ID (default 1)
 *   CONTRIBUTOR_SOLANA_PUBKEY
 *   CONTRACT_ID — if set, call the on-chain view first
 *   NEAR_PREDECESSOR — used when CONTRACT_ID is unset (default example.testnet)
 *   MPC_CONTRACT (default v1.signer-prod.testnet)
 */
import { JsonRpcProvider } from "near-api-js";
import {
  MPC_CONTRACT,
  TESTNET_RPC,
  solanaAddressFromNearKey,
  viewDerivedPublicKey,
} from "./near-rpc.ts";

export const DEFAULT_TRUST_ID = "1";
export const DEFAULT_CONTRIBUTOR =
  process.env.CONTRIBUTOR_SOLANA_PUBKEY ??
  "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";

export type DepositAddressView = {
  trust_id: number;
  contributor: string;
  path: string;
  predecessor: string;
  mpc_contract: string;
  domain_id: number;
};

export function depositPath(trustId: string | number, contributor: string): string {
  return `trust/${trustId}/in/${contributor}`;
}

export function localDepositAddressView(args: {
  trustId: string | number;
  contributor: string;
  predecessor: string;
  mpcContract?: string;
}): DepositAddressView {
  return {
    trust_id: Number(args.trustId),
    contributor: args.contributor,
    path: depositPath(args.trustId, args.contributor),
    predecessor: args.predecessor,
    mpc_contract: args.mpcContract ?? MPC_CONTRACT,
    domain_id: 1,
  };
}

export async function viewDepositAddress(args: {
  contractId: string;
  trustId: string | number;
  contributor: string;
}): Promise<DepositAddressView> {
  const provider = new JsonRpcProvider({ url: TESTNET_RPC });
  const result = await provider.callFunction({
    contractId: args.contractId,
    method: "deposit_address",
    args: {
      trust_id: Number(args.trustId),
      contributor_solana_pubkey: args.contributor,
    },
  });
  return result as DepositAddressView;
}

export async function resolveDepositAddress(options?: {
  trustId?: string | number;
  contributor?: string;
  contractId?: string;
  predecessor?: string;
}): Promise<DepositAddressView & { near_key: string; solana_address: string }> {
  const trustId = options?.trustId ?? process.env.TRUST_ID ?? DEFAULT_TRUST_ID;
  const contributor = options?.contributor ?? DEFAULT_CONTRIBUTOR;
  const contractId = options?.contractId ?? process.env.CONTRACT_ID;
  const predecessor =
    options?.predecessor ?? process.env.NEAR_PREDECESSOR ?? "example.testnet";

  const view = contractId
    ? await viewDepositAddress({ contractId, trustId, contributor })
    : localDepositAddressView({ trustId, contributor, predecessor });

  const nearKey = await viewDerivedPublicKey({
    path: view.path,
    predecessor: view.predecessor,
    domain_id: view.domain_id,
  });

  return {
    ...view,
    near_key: nearKey,
    solana_address: solanaAddressFromNearKey(nearKey),
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const row = await resolveDepositAddress();
  console.log(JSON.stringify({ signer: MPC_CONTRACT, row }, null, 2));
}
