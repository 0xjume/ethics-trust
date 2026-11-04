/**
 * Watcher-only: `confirm_launch_outcome(message_hash, solana_sig, tokens_bought)`
 * after the launch lands. Records the raise-ATA token balance (6-decimal raw
 * units, JSON string so values above 2^53 survive), pool, and fill fee, then
 * moves the trust to Launched (or Distributed when every floor share is 0).
 *
 * Refuses unless EXECUTE_CONFIRM_LAUNCH=1.
 *
 * Env: CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY,
 *      LAUNCH_MESSAGE_HASH, SOLANA_SIG, TOKENS_BOUGHT.
 */
import { Account, JsonRpcProvider, KeyPairString, teraToGas } from "near-api-js";
import { TESTNET_RPC } from "./near-rpc.ts";

export async function runConfirmLaunch(): Promise<void> {
  const contractId = process.env.CONTRACT_ID;
  const accountId = process.env.NEAR_ACCOUNT_ID;
  const privateKey = process.env.NEAR_PRIVATE_KEY as KeyPairString | undefined;
  const messageHash = process.env.LAUNCH_MESSAGE_HASH;
  const solanaSig = process.env.SOLANA_SIG;
  const tokensBought = process.env.TOKENS_BOUGHT;
  if (
    !contractId ||
    !accountId ||
    !privateKey ||
    !messageHash ||
    !solanaSig ||
    !tokensBought
  ) {
    throw new Error(
      "Set CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY, LAUNCH_MESSAGE_HASH, SOLANA_SIG, TOKENS_BOUGHT.",
    );
  }
  if (!/^[1-9][0-9]*$/.test(tokensBought)) {
    throw new Error("TOKENS_BOUGHT must be a positive decimal integer (6-decimal raw units).");
  }

  if (process.env.EXECUTE_CONFIRM_LAUNCH !== "1") {
    console.log(
      JSON.stringify(
        {
          skipped: true,
          reason: "Set EXECUTE_CONFIRM_LAUNCH=1 to call confirm_launch_outcome.",
          contractId,
          messageHash,
          solanaSig,
          tokensBought,
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
    methodName: "confirm_launch_outcome",
    args: {
      message_hash: messageHash,
      solana_sig: solanaSig,
      tokens_bought: tokensBought,
    },
    gas: teraToGas(30),
    deposit: 1n,
  });
  console.log(
    JSON.stringify({ contractId, messageHash, solanaSig, tokensBought, result }, null, 2),
  );
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  await runConfirmLaunch();
}
