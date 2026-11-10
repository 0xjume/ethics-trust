import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  Keypair,
  Message,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import type { PlannedAction } from "./driver.ts";
import { executeAction, type NearCall } from "./execute.ts";

const SOLANA_SIG = "A".repeat(88);

function blockhash(): string {
  return Keypair.generate().publicKey.toBase58();
}

describe("executeAction", () => {
  it("reports a deposit with the balance delta and the Solana signature", async () => {
    const calls: NearCall[] = [];
    await executeAction(
      {
        type: "report_deposit",
        trustId: "7",
        state: "Raising",
        contributor: "ContributorPubkey111111111111111111111",
        lamports: 250n,
        solanaSig: SOLANA_SIG,
      },
      ports(calls),
    );
    assert.equal(calls.length, 1);
    assert.equal(calls[0]?.method, "report_deposit");
    assert.equal(calls[0]?.depositYocto, 1n);
    assert.deepEqual(calls[0]?.args, {
      trust_id: 7,
      contributor: "ContributorPubkey111111111111111111111",
      lamports: 250,
      solana_sig: SOLANA_SIG,
    });
  });

  it("requests a sweep, submits the relayer-paid transfer, and confirms it", async () => {
    const relayer = Keypair.generate();
    const vault = Keypair.generate();
    const dest = Keypair.generate();
    const hash = blockhash();
    const calls: NearCall[] = [];
    const submitted: string[] = [];
    const result = await executeAction(
      {
        type: "request_sweep",
        trustId: "3",
        state: "Filled",
        contributor: vault.publicKey.toBase58(),
        lamports: 1_000n,
        source: vault.publicKey.toBase58(),
        destination: dest.publicKey.toBase58(),
      },
      {
        relayer,
        async blockhash() {
          return { blockhash: hash, lastValidBlockHeight: 10 };
        },
        async call(req) {
          calls.push(req);
          if (req.method === "request_sweep") {
            const message = Buffer.from(String(req.args.message), "base64");
            const tx = Transaction.populate(Message.from(message));
            tx.partialSign(vault);
            const signature = tx.signatures.find((row) =>
              row.publicKey.equals(vault.publicKey),
            )?.signature;
            return { txHash: "near-sweep", value: { signature: Array.from(signature!) } };
          }
          return { txHash: "near-confirm", value: null };
        },
        async submitSolana() {
          submitted.push(SOLANA_SIG);
          return SOLANA_SIG;
        },
        async buildLaunch() {
          throw new Error("launch builder should not run");
        },
      },
    );
    assert.deepEqual(
      calls.map((call) => call.method),
      ["request_sweep", "confirm_transfer"],
    );
    assert.equal(calls[1]?.args.message_hash, result.messageHash);
    assert.equal(calls[1]?.args.solana_sig, SOLANA_SIG);
    assert.equal(calls[1]?.depositYocto, 1n);
    assert.equal(submitted.length, 1);
    assert.equal(result.nearTx, "near-confirm");
  });

  it("confirms a launch with confirm_launch_outcome and no token balance", async () => {
    const relayer = Keypair.generate();
    const raise = Keypair.generate();
    const mint = Keypair.generate();
    const dest = Keypair.generate();
    const hash = blockhash();
    const tx = new Transaction({ feePayer: relayer.publicKey, recentBlockhash: hash });
    tx.add(
      SystemProgram.transfer({
        fromPubkey: raise.publicKey,
        toPubkey: dest.publicKey,
        lamports: 1,
      }),
    );
    tx.add(
      SystemProgram.transfer({
        fromPubkey: mint.publicKey,
        toPubkey: dest.publicKey,
        lamports: 1,
      }),
    );
    const message = Buffer.from(tx.serializeMessage());
    const calls: NearCall[] = [];
    const action: PlannedAction = {
      type: "request_launch",
      trustId: "2",
      state: "Executing",
      raise: raise.publicKey.toBase58(),
      fee: dest.publicKey.toBase58(),
      mint: mint.publicKey.toBase58(),
      dbcConfig: dest.publicKey.toBase58(),
      name: "Keyless",
      symbol: "KLT",
      uri: "https://ethics.ltd/k.json",
      buyLamports: 100n,
    };
    await executeAction(action, {
      relayer,
      async blockhash() {
        return { blockhash: hash, lastValidBlockHeight: 10 };
      },
      async call(req) {
        calls.push(req);
        if (req.method !== "request_launch") return { txHash: "near-confirm", value: null };
        const unsigned = Transaction.populate(
          Message.from(Buffer.from(String(req.args.message), "base64")),
        );
        unsigned.partialSign(raise, mint);
        const sig = (key: PublicKey) =>
          Array.from(
            unsigned.signatures.find((row) => row.publicKey.equals(key))!.signature!,
          );
        return {
          txHash: "near-launch",
          value: { raise: { signature: sig(raise.publicKey) }, mint: { signature: sig(mint.publicKey) } },
        };
      },
      async submitSolana() {
        return SOLANA_SIG;
      },
      async buildLaunch() {
        return { message, blockhash: hash, lastValidBlockHeight: 10 };
      },
    });
    assert.deepEqual(
      calls.map((call) => call.method),
      ["request_launch", "confirm_launch_outcome"],
    );
    assert.equal(calls[0]?.gasTgas, 90);
    assert.equal("tokens_bought" in (calls[1]?.args ?? {}), false);
    assert.equal(calls[1]?.args.solana_sig, SOLANA_SIG);
  });
});

function ports(calls: NearCall[]) {
  return {
    relayer: Keypair.generate(),
    async blockhash() {
      return { blockhash: blockhash(), lastValidBlockHeight: 1 };
    },
    async call(req: NearCall) {
      calls.push(req);
      return { txHash: "near-report", value: null };
    },
    async submitSolana() {
      return SOLANA_SIG;
    },
    async buildLaunch() {
      throw new Error("unused");
    },
  };
}
