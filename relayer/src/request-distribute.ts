/**
 * Call `request_distribute` on the deployed Trust contract.
 * Attach 60 TGas (one MPC sign + callback + headroom) and `sign_deposit`
 * 0.01 NEAR. Permissionless: one call locks only the contributors in the
 * message (at most 5) for 150 s.
 *
 * Refuses unless EXECUTE_REQUEST_DISTRIBUTE=1.
 *
 * Env: CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY, TRUST_ID,
 *      DISTRIBUTE_MESSAGE_B64 (or runs build-distribute-message).
 */
import { Account, JsonRpcProvider, KeyPairString, nearToYocto, teraToGas } from "near-api-js";
import { buildDistributeMessage } from "./build-distribute-message.ts";
import { DISTRIBUTE_ATTACH_TGAS } from "./distribute-math.ts";
import { TESTNET_RPC } from "./near-rpc.ts";

export async function runRequestDistribute(): Promise<void> {
  const contractId = process.env.CONTRACT_ID;
  const accountId = process.env.NEAR_ACCOUNT_ID;
  const privateKey = process.env.NEAR_PRIVATE_KEY as KeyPairString | undefined;
  if (!contractId || !accountId || !privateKey) {
    throw new Error("Set CONTRACT_ID, NEAR_ACCOUNT_ID, NEAR_PRIVATE_KEY.");
  }
  const trustId = Number(process.env.TRUST_ID ?? "1");
  const built = process.env.DISTRIBUTE_MESSAGE_B64
    ? {
        messageBase64: process.env.DISTRIBUTE_MESSAGE_B64,
        messageHash: process.env.DISTRIBUTE_MESSAGE_HASH ?? null,
      }
    : await buildDistributeMessage();
  const message = (built as { messageBase64: string }).messageBase64;

  if (process.env.EXECUTE_REQUEST_DISTRIBUTE !== "1") {
    console.log(
      JSON.stringify(
        {
          skipped: true,
          reason: "Set EXECUTE_REQUEST_DISTRIBUTE=1 to call request_distribute.",
          contractId,
          trustId,
          attachTgas: DISTRIBUTE_ATTACH_TGAS,
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
    methodName: "request_distribute",
    args: { trust_id: trustId, message },
    gas: teraToGas(DISTRIBUTE_ATTACH_TGAS),
    deposit: nearToYocto("0.01"),
  });
  console.log(
    JSON.stringify(
      {
        contractId,
        trustId,
        attachTgas: DISTRIBUTE_ATTACH_TGAS,
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
  await runRequestDistribute();
}
