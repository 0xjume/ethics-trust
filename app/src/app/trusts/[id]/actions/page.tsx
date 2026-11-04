"use client";

import { useWallet } from "@solana/wallet-adapter-react";
import { Connection, PublicKey } from "@solana/web3.js";
import { asBigInt, formatNearTime, formatSol, formatTokens } from "@keyless-trust/ui/format";
import { buildDistributeCli, isSolanaPubkey } from "@keyless-trust/ui/validate";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { NearGate } from "@/components/NearGate";
import { StatusChip } from "@/components/StatusChip";
import { CONTRACT_ID, SOLANA_RPC, nearToYocto, tgas } from "@/lib/config";
import { bytesToBase64, buildDistributeMessage, buildRefundMessage, sha256Hex } from "@/lib/messages";
import {
  claimable,
  derivedPublicKey,
  depositAddress,
  getContribution,
  getPending,
  getTrust,
  isConsumed,
  messageHashesFromOutcome,
  raiseRefundable,
  refundable,
  solanaAddressFromNearKey,
  type PendingTransfer,
  type TrustView,
} from "@/lib/near";

const SIGN = {
  gas: tgas(60),
  deposit: nearToYocto("0.01"),
};

export default function ActionsPage() {
  const params = useParams<{ id: string }>();
  const trustId = Number(params.id);
  const { publicKey } = useWallet();
  const [manual, setManual] = useState("");
  const pubkey = (manual || publicKey?.toBase58() || "").trim();
  const [trust, setTrust] = useState<TrustView | null | undefined>(undefined);
  const [amounts, setAmounts] = useState<{ refund: bigint; raise: bigint; claim: bigint; deposit: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [hash, setHash] = useState("");
  const [pending, setPending] = useState<PendingTransfer | null | undefined>(undefined);
  const [consumed, setConsumed] = useState<boolean | null>(null);
  const [cli, setCli] = useState<string | null>(null);

  useEffect(() => {
    if (!Number.isInteger(trustId) || trustId <= 0) {
      setTrust(null);
      return;
    }
    let active = true;
    void getTrust(trustId)
      .then((row) => {
        if (active) setTrust(row);
      })
      .catch((err: unknown) => {
        if (active) setError(err instanceof Error ? err.message : "Could not load the Trust");
      });
    return () => {
      active = false;
    };
  }, [trustId]);

  useEffect(() => {
    if (!trust || !isSolanaPubkey(pubkey)) {
      setAmounts(null);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const [refund, raise, claim, contribution, view] = await Promise.all([
          refundable(trust.id, pubkey),
          raiseRefundable(trust.id, pubkey),
          claimable(trust.id, pubkey),
          getContribution(trust.id, pubkey),
          depositAddress(trust.id, pubkey),
        ]);
        let deposit = contribution?.deposit_address ?? null;
        if (!deposit) {
          const derived = await derivedPublicKey(view.path, view.predecessor);
          deposit = solanaAddressFromNearKey(derived);
        }
        if (!active) return;
        setAmounts({
          refund: asBigInt(refund),
          raise: asBigInt(raise),
          claim: asBigInt(claim),
          deposit,
        });
        setError(null);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Could not read contributor views");
      }
    })();
    return () => {
      active = false;
    };
  }, [trust, pubkey]);

  async function lookup(nextHash = hash) {
    const clean = nextHash.trim().toLowerCase();
    if (!/^[0-9a-f]{64}$/.test(clean)) {
      setError("Message hash must be 64 hex characters.");
      return;
    }
    setError(null);
    const [row, done] = await Promise.all([getPending(clean), isConsumed(clean).catch(() => null)]);
    setPending(row);
    setConsumed(done);
  }

  async function prepareRefund(kind: "deposit" | "raise") {
    if (!trust || !amounts || !isSolanaPubkey(pubkey)) throw new Error("Load a contributor first");
    const lamports = kind === "deposit" ? amounts.refund : amounts.raise;
    const sourceText = kind === "deposit" ? amounts.deposit : trust.raise_address;
    if (!sourceText) throw new Error("The source address is not on the contract yet");
    if (lamports <= 0n) throw new Error("Nothing to refund");
    const feePayer = new PublicKey(pubkey);
    const connection = new Connection(SOLANA_RPC, "confirmed");
    const { blockhash } = await connection.getLatestBlockhash("confirmed");
    const message = buildRefundMessage({
      feePayer,
      source: new PublicKey(sourceText),
      destination: feePayer,
      lamports,
      blockhash,
    });
    const encoded = bytesToBase64(message);
    const digest = await sha256Hex(message);
    setHash(digest);
    setCli(nearCall(kind === "deposit" ? "request_refund" : "request_raise_refund", encoded));
    setStatus("Approve the NEAR wallet promptly. The Solana blockhash inside this message expires, and the relayer still has to co-sign and submit it.");
    return {
      trust_id: trust.id,
      contributor: pubkey,
      message: encoded,
    };
  }

  async function prepareClaim() {
    if (!trust || !amounts || !isSolanaPubkey(pubkey)) throw new Error("Load a contributor first");
    if (trust.state !== "Launched") throw new Error("Claims open after the Trust is Launched");
    if (amounts.claim <= 0n) throw new Error("Nothing left to claim");
    if (!trust.raise_address || !trust.mint_address) throw new Error("Raise or mint address is missing");
    const feePayer = new PublicKey(pubkey);
    if (
      feePayer.toBase58() === trust.raise_address ||
      feePayer.toBase58() === trust.mint_address ||
      feePayer.toBase58() === trust.fee_address
    ) {
      throw new Error("The fee payer cannot be the raise, mint, or fee vault");
    }
    const connection = new Connection(SOLANA_RPC, "confirmed");
    const { blockhash } = await connection.getLatestBlockhash("confirmed");
    const message = buildDistributeMessage({
      feePayer,
      raise: new PublicKey(trust.raise_address),
      mint: new PublicKey(trust.mint_address),
      blockhash,
      legs: [{ owner: feePayer, amount: amounts.claim }],
    });
    const encoded = bytesToBase64(message);
    const digest = await sha256Hex(message);
    setHash(digest);
    setCli(
      buildDistributeCli({
        contractId: CONTRACT_ID,
        trustId: trust.id,
        messageBase64: encoded,
        accountId: "YOUR.testnet",
      }),
    );
    setStatus("request_distribute attaches 0.01 NEAR and 60 TGas. The contract signs the raise path. This page does not broadcast the Solana transaction.");
    return { trust_id: trust.id, message: encoded };
  }

  function nearCall(method: string, message: string) {
    const args = JSON.stringify({ trust_id: trustId, contributor: pubkey, message });
    return `near call ${CONTRACT_ID} ${method} '${args}' --deposit 0.01 --gas 60 Tgas --networkId testnet --accountId YOUR.testnet`;
  }

  function onOutcome(outcome: unknown) {
    const found = messageHashesFromOutcome(outcome);
    const next = found[0] ?? hash;
    if (next) {
      setHash(next);
      void lookup(next).catch((err: unknown) => {
        setError(err instanceof Error ? err.message : "Could not read the pending transfer");
      });
    }
    setStatus("NEAR accepted the request. The pending lock below is what the relayer settles. This app does not submit the signed Solana transaction.");
  }

  if (!Number.isInteger(trustId) || trustId <= 0 || trust === null) {
    return <p className="py-10 text-sm text-ethics-fog">Unknown Trust.</p>;
  }

  return (
    <section className="mx-auto w-full max-w-7xl py-8">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Contributor actions</p>
      <div className="mt-3 flex flex-wrap items-end justify-between gap-4">
        <h1 className="font-display text-4xl font-medium text-ethics-silver">
          {trust ? `${trust.name} · Trust ${trust.id}` : `Trust ${params.id}`}
        </h1>
        {trust ? <StatusChip state={trust.state} /> : null}
      </div>
      <p className="mt-3 max-w-2xl text-sm leading-relaxed text-ethics-fog">
        Refunds and claims are permissionless NEAR calls. Each one attaches 0.01 NEAR and 60 TGas. The contract signs only the vault path. A relayer co-signs as fee payer when that is your wallet, then submits on Solana devnet. Views below work with a pasted pubkey and no wallet.
      </p>

      <label className="mt-6 block text-sm text-ethics-fog">
        Solana pubkey
        <input
          className="ethics-input mt-1 w-full"
          value={manual || publicKey?.toBase58() || ""}
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => setManual(event.target.value)}
        />
      </label>
      {pubkey && !isSolanaPubkey(pubkey) ? <p className="mt-2 text-sm text-ethics-fog">That is not a Solana pubkey.</p> : null}

      {!trust ? (
        <p className="mt-6 text-sm text-ethics-fog" aria-busy="true">
          Loading Trust…
        </p>
      ) : (
        <div className="mt-6 grid gap-4 md:grid-cols-2">
          <article className="ethics-panel rounded-2xl p-5">
            <h2 className="font-display text-2xl text-ethics-silver">Refund</h2>
            <p className="mt-2 text-sm leading-relaxed text-ethics-fog">
              Deposit refund {amounts ? `${formatSol(amounts.refund)} SOL` : "—"}. Raise-vault refund {amounts ? `${formatSol(amounts.raise)} SOL` : "—"}. The amount must be the full refundable balance, and the destination is this pubkey.
            </p>
            <NearGate
              label="Request deposit refund"
              disabled={!amounts || amounts.refund <= 0n}
              call={{ contractId: CONTRACT_ID, method: "request_refund", args: {}, ...SIGN }}
              prepare={() => prepareRefund("deposit")}
              onResult={onOutcome}
              onError={setError}
            />
            <NearGate
              label="Request raise refund"
              disabled={!amounts || amounts.raise <= 0n || !trust.raise_address}
              call={{ contractId: CONTRACT_ID, method: "request_raise_refund", args: {}, ...SIGN }}
              prepare={() => prepareRefund("raise")}
              onResult={onOutcome}
              onError={setError}
            />
          </article>
          <article className="ethics-panel rounded-2xl p-5">
            <h2 className="font-display text-2xl text-ethics-silver">Claim tokens</h2>
            <p className="mt-2 text-sm leading-relaxed text-ethics-fog">
              Claimable {amounts ? formatTokens(amounts.claim) : "—"} base tokens. Open when the Trust is Launched and this share is still unpaid. The message is one CreateIdempotent plus TransferChecked, fee payer account 0, raise vault as the token authority.
            </p>
            {trust.state !== "Launched" ? (
              <p className="mt-3 text-sm text-ethics-fog">This Trust is {trust.state}. The claim button waits for Launched.</p>
            ) : null}
            <NearGate
              label="Request distribute"
              disabled={!amounts || amounts.claim <= 0n || trust.state !== "Launched"}
              call={{ contractId: CONTRACT_ID, method: "request_distribute", args: {}, ...SIGN }}
              prepare={prepareClaim}
              onResult={onOutcome}
              onError={setError}
            />
          </article>
        </div>
      )}

      <section className="ethics-panel mt-6 rounded-2xl p-5" aria-labelledby="pending-heading">
        <h2 id="pending-heading" className="font-display text-2xl text-ethics-silver">Pending transfer</h2>
        <p className="mt-2 text-sm text-ethics-fog">
          The hash is SHA-256 of the exact Solana message. get_pending shows the lock. is_consumed flips after the watcher confirm_transfer. This page does not broadcast that Solana transaction.
        </p>
        <form
          className="mt-4 flex flex-wrap items-end gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            void lookup().catch((err: unknown) => {
              setError(err instanceof Error ? err.message : "Lookup failed");
            });
          }}
        >
          <label className="block min-w-0 flex-1 text-sm text-ethics-fog">
            Message hash
            <input
              className="ethics-input mt-1 w-full"
              value={hash}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setHash(event.target.value)}
            />
          </label>
          <button
            type="submit"
            className="inline-flex items-center rounded-lg border border-ethics-slate px-5 py-2.5 text-sm font-medium text-ethics-silver hover:border-ethics-teal/40"
          >
            Look up
          </button>
        </form>
        {pending === null ? <p className="mt-4 text-sm text-ethics-fog">No pending transfer for that hash.</p> : null}
        {pending ? (
          <dl className="mt-4 space-y-2 text-sm text-ethics-fog">
            <div className="flex justify-between gap-4"><dt>Kind</dt><dd className="text-ethics-silver">{pending.kind}</dd></div>
            <div className="flex justify-between gap-4"><dt>Contributor</dt><dd className="kt-mono text-right text-ethics-silver">{pending.contributor || "—"}</dd></div>
            <div className="flex justify-between gap-4"><dt>Amount field</dt><dd>{String(pending.lamports)}</dd></div>
            <div className="flex justify-between gap-4"><dt>Expires</dt><dd>{formatNearTime(pending.expires_at_ns)}</dd></div>
            <div className="flex justify-between gap-4"><dt>Settled</dt><dd>{pending.settled ? "Yes" : "No"}</dd></div>
            <div className="flex justify-between gap-4"><dt>Released</dt><dd>{pending.released ? "Yes" : "No"}</dd></div>
            <div className="flex justify-between gap-4"><dt>Consumed</dt><dd>{consumed == null ? "—" : consumed ? "Yes" : "No"}</dd></div>
          </dl>
        ) : null}
        {cli ? <pre className="kt-mono mt-4 whitespace-pre-wrap text-ethics-fog">{cli}</pre> : null}
      </section>

      {status ? (
        <p className="mt-4 text-sm text-ethics-jade" role="status">
          {status}
        </p>
      ) : null}
      {error ? (
        <div className="kt-error mt-4" role="alert">
          {error}
        </div>
      ) : null}
      <p className="mt-6 text-sm">
        <Link href={`/trusts/${trustId}`} className="text-ethics-teal hover:text-ethics-jade">
          Back to the Trust
        </Link>
      </p>
    </section>
  );
}
