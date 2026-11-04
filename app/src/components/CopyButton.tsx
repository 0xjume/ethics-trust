"use client";

import { useState } from "react";

export function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="rounded-lg border border-ethics-slate px-3 py-1.5 text-xs text-ethics-silver hover:border-ethics-teal/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ethics-teal/60"
      onClick={() => {
        void navigator.clipboard.writeText(value).then(() => {
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "Copied" : label}
    </button>
  );
}
