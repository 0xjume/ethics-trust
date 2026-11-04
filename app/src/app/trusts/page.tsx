"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { countContributors, formatSol, sharePercent } from "@keyless-trust/ui/format";
import { StatusChip } from "@/components/StatusChip";
import { listContributorKeys, listTrusts, type TrustView } from "@/lib/near";

const FILTERS = ["All", "Raising", "Filled", "Executing", "Launched", "Distributed", "Refunding"] as const;

export default function TrustsPage() {
  const [trusts, setTrusts] = useState<TrustView[] | null>(null);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>("All");

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const [rows, keys] = await Promise.all([listTrusts(), listContributorKeys().catch(() => [])]);
        if (!active) return;
        setTrusts(rows);
        const next: Record<string, number> = {};
        for (const row of rows) next[String(row.id)] = countContributors(keys, row.id);
        setCounts(next);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Could not read Trusts");
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const visible = useMemo(
    () => (trusts ?? []).filter((trust) => filter === "All" || trust.state === filter),
    [trusts, filter],
  );

  return (
    <section className="mx-auto w-full max-w-7xl pb-8 pt-8">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Discover</p>
          <h1 className="mt-2 font-display text-3xl font-medium text-ethics-silver">Trusts you can read</h1>
          <p className="mt-2 max-w-xl text-sm text-ethics-fog">
            Read from the contract. No wallet required. Raised SOL is what the watcher has credited, not a promise of a market.
          </p>
        </div>
        <div className="flex flex-wrap gap-2" role="group" aria-label="Filter by status">
          {FILTERS.map((item) => (
            <button
              key={item}
              type="button"
              aria-pressed={filter === item}
              onClick={() => setFilter(item)}
              className={`rounded-full border px-3 py-1 text-xs uppercase tracking-wide transition ${
                filter === item
                  ? "border-ethics-teal text-ethics-teal"
                  : "border-ethics-slate text-ethics-fog hover:border-ethics-teal/40"
              }`}
            >
              {item}
            </button>
          ))}
        </div>
      </div>

      {error ? (
        <div className="kt-error mt-6" role="alert">
          <p>Could not load Trusts from testnet.</p>
          <p className="mt-1 text-ethics-fog">{error}</p>
        </div>
      ) : null}

      <div className="mt-6 grid gap-4 md:grid-cols-2" aria-busy={trusts == null && !error}>
        {trusts == null && !error ? <p className="text-sm text-ethics-fog">Loading Trusts…</p> : null}
        {trusts && visible.length === 0 ? (
          <p className="text-sm text-ethics-fog">
            {trusts.length === 0 ? "No Trusts on this contract yet." : "No Trusts in this state."}
          </p>
        ) : null}
        {visible.map((trust) => {
          const raised = formatSol(trust.credited_total);
          const target = formatSol(trust.target_lamports);
          const pct = sharePercent(trust.credited_total, trust.target_lamports);
          const width = Math.min(100, Number(pct));
          return (
            <article key={trust.id} className="ethics-panel rounded-2xl p-5">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Trust {trust.id}</p>
                  <h2 className="mt-2 font-display text-2xl text-ethics-silver">
                    {trust.name} <span className="text-ethics-fog">{trust.symbol}</span>
                  </h2>
                </div>
                <StatusChip state={trust.state} />
              </div>
              <p className="mt-4 font-display text-3xl text-ethics-silver">
                {raised} <span className="text-lg text-ethics-fog">/ {target} SOL</span>
              </p>
              <div className="kt-progress mt-3" aria-hidden="true">
                <span style={{ width: `${Number.isFinite(width) ? width : 0}%` }} />
              </div>
              <p className="mt-2 text-sm text-ethics-fog">
                {pct}% of target · {counts[String(trust.id)] ?? 0} contributors · cap {trust.cap_bps / 100}%
              </p>
              <Link href={`/trusts/${trust.id}`} className="mt-4 inline-flex text-sm text-ethics-teal hover:text-ethics-jade">
                Open Trust →
              </Link>
            </article>
          );
        })}
      </div>
    </section>
  );
}
