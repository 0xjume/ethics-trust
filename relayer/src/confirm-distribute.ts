/**
 * Watcher-only: `confirm_transfer(message_hash, solana_sig)` after the
 * distribute tx lands. Books each leg's tokens and moves the trust to
 * Distributed once every floor share is paid. Dust stays in the raise ATA.
 *
 * Refuses unless EXECUTE_CONFIRM_DISTRIBUTE=1.
 *
 * Env: CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY,
 *      DISTRIBUTE_MESSAGE_HASH, SOLANA_SIG.
 */
import { Account, JsonRpcProvider, KeyPairString, teraToGas } from "near-api-js";
import { TESTNET_RPC } from "./near-rpc.ts";

export async function runConfirmDistribute(): Promise<void> {
  const contractId = process.env.CONTRACT_ID;
  const accountId = process.env.NEAR_ACCOUNT_ID;
  const privateKey = process.env.NEAR_PRIVATE_KEY as KeyPairString | undefined;
  const messageHash = process.env.DISTRIBUTE_MESSAGE_HASH;
  const solanaSig = process.env.SOLANA_SIG;
  if (!contractId || !accountId || !privateKey || !messageHash || !solanaSig) {
    throw new Error(
      "Set CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY, DISTRIBUTE_MESSAGE_HASH, SOLANA_SIG.",
    );
  }

  if (process.env.EXECUTE_CONFIRM_DISTRIBUTE !== "1") {
    console.log(
      JSON.stringify(
        {
          skipped: true,
          reason: "Set EXECUTE_CONFIRM_DISTRIBUTE=1 to call confirm_transfer.",
          contractId,
          messageHash,
          solanaSig,
        },
        null,
        2,
      ),
    );
    return;
  }

  const provider = new JsonRpcProvider({
    url: process.env.NEAR_RPC_URL ?? TESTNET_RPC,
  });
  const account = new Account(accountId, provider, privateKey);
  const result = await account.callFunction({
    contractId,
    methodName: "confirm_transfer",
    args: { message_hash: messageHash, solana_sig: solanaSig },
    gas: teraToGas(30),
    deposit: 1n,
  });
  console.log(JSON.stringify({ contractId, messageHash, solanaSig, result }, null, 2));
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  await runConfirmDistribute();
}
