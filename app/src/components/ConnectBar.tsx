"use client";

import { useWallet } from "@solana/wallet-adapter-react";
import dynamic from "next/dynamic";
import { useContext, useState } from "react";
import { shortKey } from "@keyless-trust/ui/format";
import { NearReadyContext } from "./Providers";

const NearAccountButton = dynamic(
  () => import("./NearAccountButton").then((mod) => mod.NearAccountButton),
  { ssr: false },
);

export function ConnectBar() {
  const nearReady = useContext(NearReadyContext);
  const { publicKey, connected, connecting, disconnect, wallets, select, connect } = useWallet();
  const [open, setOpen] = useState(false);
  const label = connected && publicKey ? shortKey(publicKey.toBase58(), 4, 4) : connecting ? "Connecting…" : "Solana";

  return (
    <div className="flex items-center gap-2">
      {nearReady ? <NearAccountButton /> : <span className="rounded-lg border border-ethics-slate px-3 py-2 text-xs text-ethics-fog">NEAR…</span>}
      <div className="relative">
        <button
          type="button"
          className="rounded-lg border border-ethics-slate px-3 py-2 text-sm text-ethics-silver transition hover:border-ethics-teal/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ethics-teal/60"
          aria-expanded={open}
          aria-haspopup="menu"
          onClick={() => {
            if (connected) {
              void disconnect();
              setOpen(false);
              return;
            }
            setOpen((value) => !value);
          }}
        >
          {label}
        </button>
        {open && !connected ? (
          <ul
            role="menu"
            className="absolute right-0 z-50 mt-2 w-56 rounded-lg border border-ethics-slate bg-ethics-night/95 p-1 shadow-lg"
          >
            {wallets.length === 0 ? (
              <li className="px-3 py-2 text-xs leading-relaxed text-ethics-fog">
                No Wallet Standard wallet detected. Install Phantom or Solflare and set it to Devnet.
              </li>
            ) : (
              wallets.map((wallet) => (
                <li key={wallet.adapter.name}>
                  <button
                    type="button"
                    role="menuitem"
                    className="w-full rounded-md px-3 py-2 text-left text-sm text-ethics-silver hover:bg-ethics-slate/40"
                    onClick={() => {
                      setOpen(false);
                      void (async () => {
                        select(wallet.adapter.name);
                        await connect().catch(() => undefined);
                      })();
                    }}
                  >
                    {wallet.adapter.name}
                  </button>
                </li>
              ))
            )}
          </ul>
        ) : null}
      </div>
    </div>
  );
}
