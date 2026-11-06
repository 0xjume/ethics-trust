import { PublicKey } from "@solana/web3.js";
import { buildRelayerPaidVaultTransfer } from "./near-signed-solana-transfer.ts";

/** Legacy System transfer. Relayer pays the fee. The vault is the source and a signer. */
export function systemTransferMessage(args: {
  relayer: PublicKey;
  source: PublicKey;
  destination: PublicKey;
  lamports: bigint;
  blockhash: string;
}): Buffer {
  if (args.lamports <= 0n || args.lamports > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("transfer lamports must fit a Solana system transfer");
  }
  const tx = buildRelayerPaidVaultTransfer({
    relayer: args.relayer,
    vault: args.source,
    destination: args.destination,
    lamports: Number(args.lamports),
    blockhash: args.blockhash,
  });
  const message = tx.serializeMessage();
  if (message[0]! & 0x80) throw new Error("versioned message is not allowed");
  return Buffer.from(message);
}
