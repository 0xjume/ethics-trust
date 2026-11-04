"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import {
  formatNearTime,
  formatSol,
  formatTokens,
  sharePercent,
  solanaExplorerAddress,
} from "@keyless-trust/ui/format";
import { CopyButton } from "@/components/CopyButton";
import { StatusChip } from "@/components/StatusChip";
import {
  claimable,
  distributionStatus,
  fillFee,
  getContribution,
  getTrust,
  listContributorKeys,
  type Contribution,
  type DistributionStatus,
  type TrustView,
} from "@/lib/near";

type Row = {
  contribution: Contribution;
  claimable: string | number;
};

const TIMELINE = ["Raising", "Filled", "Executing", "Launched", "Distributed"] as const;

export default function TrustDetailPage() {
  const params = useParams<{ id: string }>();
  const trustId = Number(params.id);
  const [trust, setTrust] = useState<TrustView | null | undefined>(undefined);
  const [fee, setFee] = useState<string | number | null>(null);
  const [status, setStatus] = useState<DistributionStatus | null>(null);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!Number.isInteger(trustId) || trustId <= 0) {
      setTrust(null);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const [nextTrust, nextFee, nextStatus, keys] = await Promise.all([
          getTrust(trustId),
          fillFee(trustId).catch(() => null),
          distributionStatus(trustId).catch(() => null),
          listContributorKeys().catch(() => []),
        ]);
        if (!active) return;
        setTrust(nextTrust);
        setFee(nextFee);
        setStatus(nextStatus);
        const mine = keys.filter((key) => key.trustId === String(trustId));
        const loaded = await Promise.all(
          mine.map(async (key) => {
            const [contribution, remaining] = await Promise.all([
              getContribution(trustId, key.contributor),
              claimable(trustId, key.contributor).catch(() => 0),
            ]);
            return contribution ? { contribution, claimable: remaining } : null;
          }),
        );
        if (active) setRows(loaded.filter((row): row is Row => row != null));
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Could not read this Trust");
      }
    })();
    return () => {
      active = false;
    };
  }, [trustId]);

  if (!Number.isInteger(trustId) || trustId <= 0 || trust === null) {
    return (
      <section className="py-10">
        <div className="kt-error" role="alert">
          <p>This Trust is not on the contract.</p>
          <Link href="/trusts" className="mt-2 inline-flex text-ethics-teal">
            Back to Trusts
          </Link>
        </div>
      </section>
    );
  }

  if (error) {
    return (
      <section className="py-10">
        <div className="kt-error" role="alert">
          <p>Could not load Trust {trustId}.</p>
          <p className="mt-1 text-ethics-fog">{error}</p>
        </div>
      </section>
    );
  }

  if (!trust) {
    return (
      <p className="py-10 text-sm text-ethics-fog" aria-busy="true">
        Loading Trust {params.id}…
      </p>
    );
  }

  const pct = sharePercent(trust.credited_total, trust.target_lamports);
  const width = Math.min(100, Number(pct));
  const buy =
    fee == null
      ? null
      : BigInt(trust.credited_total) > BigInt(fee)
        ? BigInt(trust.credited_total) - BigInt(fee)
        : 0n;
  const quoted = BigInt(trust.tokens_bought);
  const timeline = trust.state === "Refunding" ? (["Raising", "Refunding"] as const) : TIMELINE;

  return (
    <article className="mx-auto w-full max-w-7xl py-8">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Trust {trust.id}</p>
      <div className="mt-3 flex flex-wrap items-end justify-between gap-4">
        <h1 className="font-display text-4xl font-medium tracking-tight text-ethics-silver">
          {trust.name} <span className="text-ethics-fog">{trust.symbol}</span>
        </h1>
        <StatusChip state={trust.state} />
      </div>
      <p className="mt-3 max-w-2xl text-sm leading-relaxed text-ethics-fog">
        Watcher {trust.watcher}. Cap {trust.cap_bps} bps. Fill fee {trust.fill_fee_bps} bps. Deadline {formatNearTime(trust.deadline_ns)}.
      </p>
      <div className="mt-5 flex flex-wrap gap-3">
        <Link href={`/trusts/${trust.id}/contribute`} className="inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade">
          Contribute
        </Link>
        <Link href={`/trusts/${trust.id}/actions`} className="inline-flex items-center rounded-lg border border-ethics-slate px-5 py-2.5 text-sm font-medium text-ethics-silver hover:border-ethics-teal/40">
          Refund or claim
        </Link>
      </div>

      <section className="ethics-panel mt-8 rounded-2xl p-5" aria-labelledby="progress-heading">
        <h2 id="progress-heading" className="font-display text-2xl text-ethics-silver">
          {formatSol(trust.credited_total)} <span className="text-lg text-ethics-fog">/ {formatSol(trust.target_lamports)} SOL</span>
        </h2>
        <div className="kt-progress mt-4" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Number.isFinite(width) ? width : 0} aria-label="Raised versus target">
          <span style={{ width: `${Number.isFinite(width) ? width : 0}%` }} />
        </div>
        <p className="mt-2 text-sm text-ethics-fog">{pct}% of target · overflow {formatSol(trust.overflow_total)} SOL</p>
      </section>

      <section className="mt-6" aria-labelledby="address-heading">
        <h2 id="address-heading" className="font-display text-2xl text-ethics-silver">Derived addresses</h2>
        <p className="mt-2 text-sm text-ethics-fog">Solana explorer links are devnet. Deposit addresses are per contributor, on the contribute page.</p>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <AddressCard label="Raise vault" path={trust.raise_path} address={trust.raise_address} />
          <AddressCard label="Fee vault" path={trust.fee_path} address={trust.fee_address} />
          <AddressCard label="Mint" path={trust.mint_path} address={trust.mint_address} />
          <AddressCard label="Pool" path="DBC pool" address={trust.pool_address} />
        </div>
      </section>

      <section className="mt-8 grid gap-4 md:grid-cols-2">
        <div className="ethics-panel rounded-2xl p-5">
          <h2 className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Curve and params hash</h2>
          <p className="kt-mono mt-3 text-ethics-silver">{trust.params_hash}</p>
          <dl className="mt-4 space-y-2 text-sm text-ethics-fog">
            <div className="flex justify-between gap-4"><dt>DBC config</dt><dd className="kt-mono text-right text-ethics-silver">{trust.dbc_config}</dd></div>
            <div className="flex justify-between gap-4"><dt>Sqrt start</dt><dd>{trust.curve.sqrt_start_price}</dd></div>
            <div className="flex justify-between gap-4"><dt>Migration threshold</dt><dd>{String(trust.curve.migration_quote_threshold)} lamports</dd></div>
            <div className="flex justify-between gap-4"><dt>Fee mode</dt><dd>{trust.curve.collect_fee_mode === 0 ? "Quote token" : "Output token"}</dd></div>
            <div className="flex justify-between gap-4"><dt>First swap min fee</dt><dd>{trust.curve.enable_first_swap_with_min_fee ? "On" : "Off"}</dd></div>
            <div className="flex justify-between gap-4"><dt>Cliff numerator</dt><dd>{String(trust.curve.cliff_fee_numerator)}</dd></div>
            <div className="flex justify-between gap-4"><dt>Points</dt><dd>{trust.curve.points.length}</dd></div>
            <div className="flex justify-between gap-4"><dt>URI</dt><dd className="text-right">{trust.uri}</dd></div>
          </dl>
        </div>
        <div className="ethics-panel rounded-2xl p-5">
          <h2 className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">First-buy quote</h2>
          <p className="mt-3 font-display text-3xl text-ethics-silver">
            {quoted > 0n ? formatTokens(quoted) : "0"}
          </p>
          <p className="mt-1 text-sm text-ethics-fog">
            Raw base units stored as <span className="text-ethics-silver">tokens_bought</span>. The contract writes this at <span className="text-ethics-silver">request_launch</span> from the pinned curve. Confirm does not change it.
          </p>
          <dl className="mt-4 space-y-2 text-sm text-ethics-fog">
            <div className="flex justify-between gap-4"><dt>Fill fee</dt><dd>{fee == null ? "—" : `${formatSol(fee)} SOL`}</dd></div>
            <div className="flex justify-between gap-4"><dt>Buy size</dt><dd>{buy == null ? "—" : `${formatSol(buy)} SOL`}</dd></div>
            <div className="flex justify-between gap-4"><dt>Claimable total</dt><dd>{formatTokens(trust.claimable_total)}</dd></div>
            <div className="flex justify-between gap-4"><dt>Distributed</dt><dd>{formatTokens(trust.distributed_total)}</dd></div>
            <div className="flex justify-between gap-4"><dt>Dust left in the raise</dt><dd>{status ? formatTokens(status.dust_tokens) : "—"}</dd></div>
          </dl>
        </div>
      </section>

      <section className="mt-8" aria-labelledby="timeline-heading">
        <h2 id="timeline-heading" className="font-display text-2xl text-ethics-silver">State</h2>
        <ol className="mt-4 grid gap-3 md:grid-cols-3">
          {timeline.map((step) => {
            const current = step === trust.state;
            return (
              <li key={step} className={`ethics-panel rounded-2xl p-4 ${current ? "border-ethics-teal" : ""}`} aria-current={current ? "step" : undefined}>
                <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">{current ? "Now" : "State"}</p>
                <p className="mt-2 font-display text-xl text-ethics-silver">{step}</p>
                <p className="mt-1 text-xs text-ethics-fog">{stamp(step, trust)}</p>
              </li>
            );
          })}
        </ol>
      </section>

      <section className="mt-8" aria-labelledby="who-heading">
        <h2 id="who-heading" className="font-display text-2xl text-ethics-silver">Who can move funds</h2>
        <ul className="mt-4 grid gap-3 md:grid-cols-2 text-sm leading-relaxed text-ethics-fog">
          <li className="ethics-panel rounded-2xl p-4">Anyone can ask for a full refund to the contributor&apos;s own pubkey, when the view says there is something to refund.</li>
          <li className="ethics-panel rounded-2xl p-4">Only the watcher can sweep, pay the fill fee, or request the launch. The cap and the curve still bind them.</li>
          <li className="ethics-panel rounded-2xl p-4">Anyone can request a pro-rata claim in Launched, for the remaining floor share only, with 0.01 NEAR and 60 TGas.</li>
          <li className="ethics-panel rounded-2xl p-4">No full-access key on the contract. See Verify for the live key list.</li>
        </ul>
      </section>

      <section className="mt-8" aria-labelledby="contributors-heading">
        <h2 id="contributors-heading" className="font-display text-2xl text-ethics-silver">Contributors</h2>
        <p className="mt-2 text-sm text-ethics-fog">Read from contract state. Share is credited SOL over the credited total.</p>
        {rows == null ? (
          <p className="mt-4 text-sm text-ethics-fog" aria-busy="true">
            Loading contributors…
          </p>
        ) : rows.length === 0 ? (
          <p className="mt-4 text-sm text-ethics-fog">No registered contributors in contract state yet.</p>
        ) : (
          <div className="mt-4 overflow-x-auto">
            <table className="kt-table">
              <caption className="sr-only">Contributors for Trust {trust.id}</caption>
              <thead>
                <tr>
                  <th scope="col">Contributor</th>
                  <th scope="col">Credited</th>
                  <th scope="col">Share</th>
                  <th scope="col">Claimable</th>
                  <th scope="col">Distributed</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.contribution.contributor}>
                    <th scope="row" className="kt-mono text-left font-normal text-ethics-silver">{row.contribution.contributor}</th>
                    <td>{formatSol(row.contribution.credited_lamports)} SOL</td>
                    <td>{sharePercent(row.contribution.credited_lamports, trust.credited_total)}%</td>
                    <td>{formatTokens(row.claimable)}</td>
                    <td>{formatTokens(row.contribution.distributed_tokens)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </article>
  );
}

function AddressCard({ label, path, address }: { label: string; path: string; address: string | null }) {
  return (
    <div className="ethics-panel rounded-2xl p-4">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">{label}</p>
      <p className="mt-2 text-xs text-ethics-fog">{path}</p>
      {address ? (
        <>
          <a className="kt-mono mt-2 inline-block text-ethics-silver hover:text-ethics-teal" href={solanaExplorerAddress(address)}>
            {address}
          </a>
          <div className="mt-3">
            <CopyButton value={address} label="Copy" />
          </div>
        </>
      ) : (
        <p className="mt-2 text-sm text-ethics-fog">Not derived yet.</p>
      )}
    </div>
  );
}

function stamp(step: string, trust: TrustView): string {
  if (step === "Filled") return trust.filled_at_ns ? formatNearTime(trust.filled_at_ns) : "When credited SOL reaches the target";
  if (step === "Executing") return trust.execution_started_ns ? formatNearTime(trust.execution_started_ns) : "Starts at the first sweep";
  if (step === "Launched") return trust.pool_address ? "Pool is set" : "After the create-and-buy confirms";
  if (step === "Distributed") return "Every floor share is booked";
  if (step === "Refunding") return "Deadline missed, or the execution window expired";
  return `Until ${formatNearTime(trust.deadline_ns)}`;
}
