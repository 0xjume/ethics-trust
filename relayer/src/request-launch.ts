/**
 * Call `request_launch` on the deployed Trust contract.
 * Attach 90 TGas (two MPC signs + callback) and `sign_deposit` 0.01 NEAR.
 *
 * Refuses unless EXECUTE_REQUEST_LAUNCH=1.
 *
 * Env: CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY, TRUST_ID,
 *      LAUNCH_MESSAGE_B64 (or runs build-launch-message).
 */
import { Account, JsonRpcProvider, KeyPairString, teraToGas, nearToYocto } from "near-api-js";
import { TESTNET_RPC } from "./near-rpc.ts";
import { buildLaunchMessage } from "./build-launch-message.ts";

export const LAUNCH_ATTACH_TGAS = 90;

export async function runRequestLaunch(): Promise<void> {
  const contractId = process.env.CONTRACT_ID;
  const accountId = process.env.NEAR_ACCOUNT_ID;
  const privateKey = process.env.NEAR_PRIVATE_KEY as KeyPairString | undefined;
  if (!contractId || !accountId || !privateKey) {
    throw new Error("Set CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY.");
  }
  const trustId = Number(process.env.TRUST_ID ?? "1");
  const built = process.env.LAUNCH_MESSAGE_B64
    ? {
        messageBase64: process.env.LAUNCH_MESSAGE_B64,
        messageHash: process.env.LAUNCH_MESSAGE_HASH ?? null,
      }
    : await buildLaunchMessage();
  const message = (built as { messageBase64: string }).messageBase64;

  if (process.env.EXECUTE_REQUEST_LAUNCH !== "1") {
    console.log(
      JSON.stringify(
        {
          skipped: true,
          reason: "Set EXECUTE_REQUEST_LAUNCH=1 to call request_launch.",
          contractId,
          trustId,
          attachTgas: LAUNCH_ATTACH_TGAS,
          signDepositNear: "0.01",
          messageHash: (built as { messageHash?: string }).messageHash ?? null,
          messageBytes: Buffer.from(message, "base64").length,
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
    methodName: "request_launch",
    args: { trust_id: trustId, message },
    gas: teraToGas(LAUNCH_ATTACH_TGAS),
    deposit: nearToYocto("0.01"),
  });
  console.log(
    JSON.stringify(
      {
        contractId,
        trustId,
        attachTgas: LAUNCH_ATTACH_TGAS,
        messageHash: (built as { messageHash?: string }).messageHash ?? null,
        result,
      },
      null,
      2,
    ),
  );
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  await runRequestLaunch();
}
