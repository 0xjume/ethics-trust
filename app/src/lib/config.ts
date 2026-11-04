/** Testnet and Solana devnet only. There is no mainnet switch. */

export const CONTRACT_ID =
  process.env.NEXT_PUBLIC_CONTRACT_ID ?? "m10.keyless-trust-dev-54ef71.testnet";

export const NEAR_RPC =
  process.env.NEXT_PUBLIC_NEAR_RPC_URL ?? "https://rpc.testnet.near.org";

export const SOLANA_RPC =
  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

export const MPC_CONTRACT = "v1.signer-prod.testnet";

export const NEAR_NETWORK = "testnet" as const;
export const SOLANA_CLUSTER = "devnet" as const;

/** Live proof the Verify page links to. Testnet deploy, devnet launch. */
export const PROOF = {
  deployTx: "DthW5Pdxa6gKYFfLSc9RQSvdff2u2iFXYLPv7zReZnoK",
  launchTx:
    "HM7k5veL6ivW6L1jZn3JZ6VTyoKNoWFEubm3cKMazxaceKqSSgucXFDvd7vEhd3sfKkLyHhKCVHLu7nZ2MKj7jw",
} as const;

export function tgas(amount: number): string {
  return (BigInt(amount) * 1_000_000_000_000n).toString();
}

/** Decimal NEAR ("0.005") to yoctoNEAR. */
export function nearToYocto(near: string): string {
  const [whole, frac = ""] = near.split(".");
  if (!/^\d+$/.test(whole) || (frac && !/^\d+$/.test(frac)) || frac.length > 24) {
    throw new Error("NEAR amount is invalid");
  }
  const padded = frac.padEnd(24, "0");
  return (BigInt(whole) * 10n ** 24n + BigInt(padded)).toString();
}
