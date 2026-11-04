"use client";

import { useState } from "react";
import { useNearWallet } from "near-connect-hooks";

export type NearCall = {
  contractId: string;
  method: string;
  args: Record<string, unknown>;
  gas: string;
  deposit: string;
};

export function NearCallButton({
  label,
  pendingLabel = "Confirm in the NEAR wallet…",
  disabled,
  call,
  prepare,
  onResult,
  onError,
}: {
  label: string;
  pendingLabel?: string;
  disabled?: boolean;
  call: NearCall;
  prepare?: () => Promise<Record<string, unknown>>;
  onResult: (outcome: unknown) => void;
  onError: (message: string) => void;
}) {
  const { signedAccountId, signIn, callFunctionRaw } = useNearWallet();
  const [busy, setBusy] = useState(false);
  const connected = Boolean(signedAccountId);
  const caption = busy ? pendingLabel : !connected && !disabled ? "Connect NEAR" : label;

  return (
    <button
      type="button"
      disabled={disabled || busy}
      className="mt-4 inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade disabled:opacity-50"
      onClick={() => {
        void (async () => {
          setBusy(true);
          try {
            if (!connected) {
              await signIn();
              return;
            }
            const args = prepare ? await prepare() : call.args;
            const outcome = await callFunctionRaw({ ...call, args });
            onResult(outcome);
          } catch (err) {
            onError(err instanceof Error ? err.message : "NEAR call failed");
          } finally {
            setBusy(false);
          }
        })();
      }}
    >
      {caption}
    </button>
  );
}
