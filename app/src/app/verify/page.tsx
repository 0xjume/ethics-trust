"use client";

import { nearblocksAccount, nearblocksTx, solanaExplorerTx } from "@keyless-trust/ui/format";
import { useEffect, useState } from "react";
import { CONTRACT_ID, MPC_CONTRACT, PROOF } from "@/lib/config";
import {
  dbcProgram,
  derivationPaths,
  domainId,
  listAccessKeys,
  mpcContract,
} from "@/lib/near";

type KeyRow = { public_key?: string; access_key?: { permission?: unknown } };

function permissionLabel(permission: unknown): string {
  if (permission === "FullAccess") return "FullAccess";
  if (permission && typeof permission === "object") return "FunctionCall";
  return "Unknown";
}

export default function VerifyPage() {
  const [keys, setKeys] = useState<KeyRow[] | null>(null);
  const [meta, setMeta] = useState<{ mpc: string; domain: string; program: string } | null>(null);
  const [paths, setPaths] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const [keyList, mpc, domain, program, derived] = await Promise.all([
          listAccessKeys(),
          mpcContract(),
          domainId(),
          dbcProgram(),
          derivationPaths(1),
        ]);
        if (!active) return;
        setKeys(keyList.keys ?? []);
        setMeta({ mpc: String(mpc), domain: String(domain), program: String(program) });
        setPaths(derived);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Could not read the contract");
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const fullAccess = (keys ?? []).filter((key) => permissionLabel(key.access_key?.permission) === "FullAccess");

  return (
    <section className="mx-auto w-full max-w-7xl py-8">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Verify</p>
      <h1 className="mt-2 max-w-3xl font-display text-4xl font-medium text-ethics-silver">
        The contract has no key that can move the vaults by itself.
      </h1>
      <p className="mt-3 max-w-2xl text-sm leading-relaxed text-ethics-fog">
        Keylessness here means the NEAR account has no full-access key, and Solana vaults are Ed25519 addresses derived by Chain Signatures. The contract will only ask the MPC to sign a refund, a capped sweep, one pinned Meteora DBC create-and-buy, or a pro-rata distribution.
      </p>

      <ol className="mt-8 grid gap-4 md:grid-cols-2">
        <li className="ethics-panel rounded-2xl p-5">
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">1 · Access keys</p>
          <h2 className="mt-2 font-display text-2xl text-ethics-silver">view_access_key_list</h2>
          {keys == null && !error ? (
            <p className="mt-3 text-sm text-ethics-fog" aria-busy="true">
              Reading {CONTRACT_ID}…
            </p>
          ) : null}
          {keys ? (
            <>
              <p className="mt-3 text-sm text-ethics-fog">
                {keys.length === 0
                  ? "The account has no access keys. Nothing on NEAR can sign an arbitrary call as this contract."
                  : `${keys.length} key(s) on the account. Full-access keys: ${fullAccess.length}.`}
              </p>
              {keys.length === 0 ? (
                <p className="mt-3 text-sm text-ethics-jade" role="status">
                  No full-access keys.
                </p>
              ) : (
                <ul className="mt-3 space-y-2">
                  {keys.map((key) => (
                    <li key={key.public_key ?? permissionLabel(key.access_key?.permission)} className="text-sm text-ethics-fog">
                      <span className="text-ethics-silver">{permissionLabel(key.access_key?.permission)}</span>
                      <span className="kt-mono mt-1 block">{key.public_key}</span>
                    </li>
                  ))}
                </ul>
              )}
            </>
          ) : null}
          <a className="mt-4 inline-flex text-sm text-ethics-teal hover:text-ethics-jade" href={nearblocksAccount(CONTRACT_ID)}>
            {CONTRACT_ID} on NEARBlocks testnet
          </a>
        </li>
        <li className="ethics-panel rounded-2xl p-5">
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">2 · MPC</p>
          <h2 className="mt-2 font-display text-2xl text-ethics-silver">Chain Signatures</h2>
          <dl className="mt-3 space-y-2 text-sm text-ethics-fog">
            <div className="flex justify-between gap-4">
              <dt>Signer</dt>
              <dd className="text-right text-ethics-silver">{meta?.mpc ?? MPC_CONTRACT}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt>Domain</dt>
              <dd>{meta ? `${meta.domain} · Ed25519` : "1 · Ed25519"}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt>DBC program</dt>
              <dd className="kt-mono text-right text-ethics-silver">{meta?.program ?? "—"}</dd>
            </div>
          </dl>
          <p className="mt-3 text-sm leading-relaxed text-ethics-fog">
            Paths come from the contract. The predecessor is the contract id. domain_id 1 selects Ed25519. The Solana address is the derived public key with the ed25519: prefix removed.
          </p>
        </li>
        <li className="ethics-panel rounded-2xl p-5">
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">3 · Paths for Trust 1</p>
          <h2 className="mt-2 font-display text-2xl text-ethics-silver">derivation_paths</h2>
          {paths == null && !error ? <p className="mt-3 text-sm text-ethics-fog">Loading paths…</p> : null}
          {paths ? (
            <ul className="mt-3 space-y-2">
              {paths.map((path) => (
                <li key={path} className="kt-mono text-ethics-silver">
                  {path}
                </li>
              ))}
            </ul>
          ) : null}
          <p className="mt-3 text-sm text-ethics-fog">A contributor path is trust/&lt;id&gt;/in/&lt;pubkey&gt; and appears once you pass that pubkey.</p>
        </li>
        <li className="ethics-panel rounded-2xl p-5">
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">4 · Live transactions</p>
          <h2 className="mt-2 font-display text-2xl text-ethics-silver">Deploy and launch</h2>
          <ul className="mt-3 space-y-3 text-sm">
            <li>
              <a className="text-ethics-teal hover:text-ethics-jade" href={nearblocksTx(PROOF.deployTx)}>
                Testnet deploy {PROOF.deployTx}
              </a>
            </li>
            <li>
              <a className="text-ethics-teal hover:text-ethics-jade" href={solanaExplorerTx(PROOF.launchTx)}>
                Devnet launch {PROOF.launchTx}
              </a>
            </li>
          </ul>
          <p className="mt-3 text-sm leading-relaxed text-ethics-fog">
            The launch transaction is the Meteora DBC create-and-buy signed for the m10 raise vault. The relayer paid the fee. The vault addresses are the derived keys, not a keypair in this app.
          </p>
        </li>
      </ol>

      {error ? (
        <div className="kt-error mt-4" role="alert">
          {error}
        </div>
      ) : null}

      <div className="kt-note mt-8" role="note">
        <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">What this does not prove</p>
        <p className="mt-3">
          The MPC signer set could collectively sign any payload. That is the Chain Signatures assumption. A watcher can delay a raise or skew credits among registered contributors. It cannot add a destination the contract did not pin. This page reads public RPC data. It does not hold a key.
        </p>
      </div>
    </section>
  );
}
