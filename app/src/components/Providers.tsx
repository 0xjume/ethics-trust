"use client";

import { Buffer } from "buffer";
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { createContext, useEffect, useState, type ComponentType, type ReactNode } from "react";
import { SOLANA_RPC } from "@/lib/config";

if (typeof globalThis.Buffer === "undefined") {
  globalThis.Buffer = Buffer;
}

type NearProviderProps = { config: { network: "testnet" }; children: ReactNode };

export const NearReadyContext = createContext(false);

export function Providers({ children }: { children: ReactNode }) {
  const [NearProvider, setNearProvider] = useState<ComponentType<NearProviderProps> | null>(null);

  useEffect(() => {
    let active = true;
    void import("near-connect-hooks")
      .then((mod) => {
        if (active) setNearProvider(() => mod.NearProvider as ComponentType<NearProviderProps>);
      })
      .catch(() => {
        if (active) setNearProvider(null);
      });
    return () => {
      active = false;
    };
  }, []);

  const solana = (
    <ConnectionProvider endpoint={SOLANA_RPC}>
      <WalletProvider wallets={[]} autoConnect>
        {children}
      </WalletProvider>
    </ConnectionProvider>
  );

  if (!NearProvider) {
    return <NearReadyContext.Provider value={false}>{solana}</NearReadyContext.Provider>;
  }

  return (
    <NearReadyContext.Provider value={true}>
      <NearProvider config={{ network: "testnet" }}>{solana}</NearProvider>
    </NearReadyContext.Provider>
  );
}
