"use client";

import dynamic from "next/dynamic";
import { useContext } from "react";
import { NearReadyContext } from "./Providers";
import type { NearCall } from "./NearCallButton";

const NearCallButton = dynamic(
  () => import("./NearCallButton").then((mod) => mod.NearCallButton),
  { ssr: false },
);

export function NearGate(props: {
  label: string;
  pendingLabel?: string;
  disabled?: boolean;
  call: NearCall;
  prepare?: () => Promise<Record<string, unknown>>;
  onResult: (outcome: unknown) => void;
  onError: (message: string) => void;
}) {
  const ready = useContext(NearReadyContext);
  if (!ready) {
    return (
      <p className="mt-3 text-sm text-ethics-fog">
        NEAR wallet selector is still loading. The command below is the same call.
      </p>
    );
  }
  return <NearCallButton {...props} />;
}
