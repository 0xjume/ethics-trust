import React from "react";
import { statusLabel } from "@keyless-trust/ui/format";

const TONE: Record<string, string> = {
  Raising: "border-ethics-teal text-ethics-teal",
  Filled: "border-ethics-jade text-ethics-jade",
  Executing: "kt-chip-executing",
  Launched: "border-ethics-jade text-ethics-jade",
  Distributed: "kt-chip-distributed",
  Refunding: "border-rose-500/40 text-rose",
};

export function StatusChip({ state }: { state: string }) {
  const label = statusLabel(state);
  const tone = TONE[label] ?? "border-ethics-slate text-ethics-fog";
  return (
    <span
      className={`inline-flex items-center rounded-full border px-3 py-1 text-xs uppercase tracking-wide ${tone}`}
    >
      {label}
    </span>
  );
}
