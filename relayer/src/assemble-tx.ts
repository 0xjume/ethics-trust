import { Keypair, Message, PublicKey, Transaction } from "@solana/web3.js";

export function assembleRelayerTransaction(args: {
  message: Uint8Array;
  vaultSignatures: Array<{ address: string; signature: Uint8Array }>;
  relayer: Keypair;
}): Transaction {
  const tx = Transaction.populate(Message.from(Buffer.from(args.message)));
  for (const sig of args.vaultSignatures) {
    tx.addSignature(new PublicKey(sig.address), Buffer.from(sig.signature));
  }
  tx.partialSign(args.relayer);
  if (!tx.verifySignatures()) {
    throw new Error("assembled Solana signatures failed local verify");
  }
  return tx;
}
