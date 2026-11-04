"use client";

import { useNearWallet } from "near-connect-hooks";
import { shortKey } from "@keyless-trust/ui/format";

export function NearAccountButton() {
  const { signedAccountId, signIn, signOut, loading } = useNearWallet();
  const connected = Boolean(signedAccountId);
  const label = loading ? "NEAR…" : connected ? shortKey(signedAccountId, 6, 4) : "NEAR";

  return (
    <button
      type="button"
      className="rounded-lg border border-ethics-slate px-3 py-2 text-sm text-ethics-silver transition hover:border-ethics-teal/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ethics-teal/60"
      onClick={() => {
        if (connected) void signOut();
        else void signIn();
      }}
    >
      {label}
    </button>
  );
}
