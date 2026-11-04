/**
 * Read-only: call v1.signer-prod.testnet.derived_public_key for the Trust paths.
 * No account is created. Predecessor may be any string; default matches the plan's
 * verified example.testnet call.
 */
import {
  MPC_CONTRACT,
  solanaAddressFromNearKey,
  viewDerivedPublicKey,
} from "./near-rpc.ts";

const PREDECESSOR = process.env.NEAR_PREDECESSOR ?? "example.testnet";
const TRUST_ID = process.env.TRUST_ID ?? "1";
const CONTRIBUTOR =
  process.env.CONTRIBUTOR_SOLANA_PUBKEY ??
  "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";

const PATHS = [
  `trust/${TRUST_ID}/raise`,
  `trust/${TRUST_ID}/fees`,
  `trust/${TRUST_ID}/mint`,
  `trust/${TRUST_ID}/in/${CONTRIBUTOR}`,
];

export async function deriveTrustAddresses(options?: {
  predecessor?: string;
  paths?: string[];
}) {
  const predecessor = options?.predecessor ?? PREDECESSOR;
  const paths = options?.paths ?? PATHS;
  const rows = [];
  for (const path of paths) {
    const nearKey = await viewDerivedPublicKey({
      path,
      predecessor,
      domain_id: 1,
    });
    rows.push({
      path,
      predecessor,
      domain_id: 1,
      mpc: MPC_CONTRACT,
      near_key: nearKey,
      solana_address: solanaAddressFromNearKey(nearKey),
    });
  }
  return rows;
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const rows = await deriveTrustAddresses();
  console.log(JSON.stringify({ signer: MPC_CONTRACT, rows }, null, 2));
}
