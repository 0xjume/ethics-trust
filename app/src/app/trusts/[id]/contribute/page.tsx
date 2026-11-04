"use client";

import { useWallet } from "@solana/wallet-adapter-react";
import { Connection, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";
import { buildRegisterCli, isSolanaPubkey } from "@keyless-trust/ui/validate";
import { formatSol, solanaExplorerAddress, solanaExplorerTx, solToLamports } from "@keyless-trust/ui/format";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { CopyButton } from "@/components/CopyButton";
import { NearGate } from "@/components/NearGate";
import { CONTRACT_ID, SOLANA_RPC, nearToYocto, tgas } from "@/lib/config";
import {
  depositAddress,
  derivedPublicKey,
  getContribution,
  getTrust,
  solanaAddressFromNearKey,
  type TrustView,
} from "@/lib/near";

export default function ContributePage() {
  const params = useParams<{ id: string }>();
  const trustId = Number(params.id);
  const { publicKey, connected, sendTransaction } = useWallet();
  const connectedPubkey = publicKey?.toBase58() ?? "";
  const [manual, setManual] = useState("");
  const pubkey = isSolanaPubkey(manual) ? manual.trim() : connectedPubkey;
  const [trust, setTrust] = useState<TrustView | null | undefined>(undefined);
  const [path, setPath] = useState<string | null>(null);
  const [address, setAddress] = useState<string | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [registered, setRegistered] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sol, setSol] = useState("0.01");
  const [tx, setTx] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);

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
      setPath(null);
      setAddress(null);
      setRegistered(false);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const view = await depositAddress(trust.id, pubkey);
        const derived = await derivedPublicKey(view.path, view.predecessor);
        const contribution = await getContribution(trust.id, pubkey);
        if (!active) return;
        setPath(view.path);
        setAddress(contribution?.deposit_address ?? solanaAddressFromNearKey(derived));
        setRegistered(Boolean(contribution));
        setError(null);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Could not derive the deposit address");
      }
    })();
    return () => {
      active = false;
    };
  }, [trust, pubkey, reload]);

  useEffect(() => {
    if (!address) return;
    let active = true;
    void import("qrcode").then(async (QR) => {
      const svg = await QR.toString(address, {
        type: "svg",
        margin: 1,
        color: { dark: "#d9dee3", light: "#0b0d0f" },
      });
      if (active && svg.trimStart().startsWith("<svg")) setQr(svg);
    });
    return () => {
      active = false;
    };
  }, [address]);

  async function sendSol() {
    if (!publicKey || !address || !registered) return;
    if (publicKey.toBase58() !== pubkey) {
      setError("Connect the Solana wallet that matches this pubkey before sending.");
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const lamports = solToLamports(sol);
      if (lamports <= 0n) throw new Error("Enter a SOL amount greater than 0");
      if (lamports > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("Amount is too large for a browser transfer");
      const connection = new Connection(SOLANA_RPC, "confirmed");
      const transaction = new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: publicKey,
          toPubkey: new PublicKey(address),
          lamports: Number(lamports),
        }),
      );
      const signature = await sendTransaction(transaction, connection);
      setTx(signature);
      setStatus("Transfer submitted on devnet. The watcher still has to report_deposit before the contract credits it.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Transfer failed");
    } finally {
      setBusy(false);
    }
  }

  if (trust === null) {
    return <p className="py-10 text-sm text-ethics-fog">Unknown Trust.</p>;
  }

  const capLamports = trust ? (BigInt(trust.target_lamports) * BigInt(trust.cap_bps)) / 10_000n : 0n;
  const cli = buildRegisterCli({
    contractId: CONTRACT_ID,
    trustId,
    solanaPubkey: pubkey || "YOUR_SOLANA_PUBKEY",
    accountId: "YOUR.testnet",
  });

  return (
    <section className="mx-auto w-full max-w-7xl py-8">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Contribute</p>
      <h1 className="mt-2 font-display text-4xl font-medium text-ethics-silver">
        {trust ? `${trust.name} · Trust ${trust.id}` : `Trust ${params.id}`}
      </h1>
      <p className="mt-3 max-w-2xl text-sm leading-relaxed text-ethics-fog">
        Register on NEAR, then send devnet SOL to the deposit address the contract derives for your wallet. The watcher credits it. This page never asks for a secret key.
      </p>
      <ol className="kt-steps mt-8">
        <li className="ethics-panel rounded-2xl p-5">
          <p className="kt-step-index">1</p>
          <h2 className="mt-3 font-display text-2xl text-ethics-silver">Solana pubkey</h2>
          <p className="mt-2 text-sm text-ethics-fog">
            {connected && connectedPubkey ? (
              <>
                Connected wallet <span className="text-ethics-silver">{connectedPubkey}</span>. Set the wallet to Devnet before you send.
              </>
            ) : (
              "Use the Solana button in the header. Wallet Standard wallets such as Phantom and Solflare appear there. You can also paste a pubkey to preview the deposit address."
            )}
          </p>
          <label className="mt-4 block text-sm text-ethics-fog">
            Pubkey
            <input
              className="ethics-input mt-1 w-full"
              value={manual || connectedPubkey}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setManual(event.target.value)}
            />
          </label>
        </li>
        <li className="ethics-panel rounded-2xl p-5">
          <p className="kt-step-index">2</p>
          <h2 className="mt-3 font-display text-2xl text-ethics-silver">Register on NEAR</h2>
          <p className="mt-2 text-sm leading-relaxed text-ethics-fog">
            <span className="text-ethics-silver">register(trust_id, solana_pubkey)</span> takes the contract&apos;s storage deposit, 0.005 NEAR. Excess is refunded. Anyone may register any pubkey, because refunds can only return to it. Closed Trusts reject new registration.
          </p>
          <NearGate
            label={registered ? "Registered" : "Register this wallet"}
            disabled={!isSolanaPubkey(pubkey) || registered}
            call={{
              contractId: CONTRACT_ID,
              method: "register",
              args: { trust_id: trustId, solana_pubkey: pubkey },
              gas: tgas(40),
              deposit: nearToYocto("0.005"),
            }}
            onResult={() => {
              setStatus("Registered. The deposit address below is the one the contract will refund.");
              setRegistered(true);
              setReload((value) => value + 1);
            }}
            onError={setError}
          />
          {registered ? <p className="mt-3 text-sm text-ethics-jade">This pubkey is registered.</p> : null}
          <pre className="kt-mono mt-4 whitespace-pre-wrap text-ethics-fog">{cli}</pre>
        </li>
        <li className="ethics-panel rounded-2xl p-5">
          <p className="kt-step-index">3</p>
          <h2 className="mt-3 font-display text-2xl text-ethics-silver">Deposit address</h2>
          {!isSolanaPubkey(pubkey) ? (
            <p className="mt-2 text-sm text-ethics-fog">Connect a Solana wallet or paste a pubkey to derive trust/{trustId}/in/&lt;pubkey&gt;.</p>
          ) : null}
          {path ? <p className="mt-2 text-xs text-ethics-fog">{path}</p> : null}
          {address ? (
            <>
              <a className="kt-mono mt-3 inline-block text-ethics-silver hover:text-ethics-teal" href={solanaExplorerAddress(address)}>
                {address}
              </a>
              <div className="mt-3 flex flex-wrap items-center gap-3">
                <CopyButton value={address} label="Copy address" />
                {qr ? <div className="kt-qr" dangerouslySetInnerHTML={{ __html: qr }} /> : null}
              </div>
              <p className="mt-3 text-sm text-ethics-fog">
                Per-wallet cap {formatSol(capLamports)} SOL ({trust?.cap_bps ?? "—"} bps of target). Extra SOL is overflow and stays refundable. It does not raise the credited total.
              </p>
              <form
                className="mt-4 flex flex-wrap items-end gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  void sendSol();
                }}
              >
                <label className="block text-sm text-ethics-fog">
                  Devnet SOL
                  <input
                    className="ethics-input mt-1"
                    inputMode="decimal"
                    value={sol}
                    onChange={(event) => setSol(event.target.value)}
                  />
                </label>
                <button
                  type="submit"
                  disabled={!connected || !registered || busy || publicKey?.toBase58() !== pubkey}
                  className="inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade disabled:opacity-50"
                >
                  {busy ? "Sending…" : "Send to deposit address"}
                </button>
              </form>
              {!registered ? (
                <p className="mt-2 text-xs text-ethics-fog">Register before sending so the contract can refund this pubkey.</p>
              ) : null}
              {tx ? (
                <p className="mt-3 text-sm">
                  <a className="text-ethics-teal hover:text-ethics-jade" href={solanaExplorerTx(tx)}>
                    Devnet transfer {tx}
                  </a>
                </p>
              ) : null}
            </>
          ) : null}
        </li>
      </ol>
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
      <p className="mt-6 text-sm text-ethics-fog">
        <Link href={`/trusts/${trustId}`} className="text-ethics-teal hover:text-ethics-jade">
          Back to the Trust
        </Link>
      </p>
    </section>
  );
}
