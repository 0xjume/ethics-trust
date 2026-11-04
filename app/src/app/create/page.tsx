"use client";

import { DEMO_CURVE, buildCreateTrustCli, stringifyNearArgs, validateCreateTrust } from "@keyless-trust/ui/validate";
import { useMemo, useState } from "react";
import { NearGate } from "@/components/NearGate";
import { toWalletArgs } from "@/lib/args";
import { CONTRACT_ID, nearToYocto, tgas } from "@/lib/config";

const WEEK_NS = 7n * 24n * 60n * 60n * 1_000_000_000n;

function defaultDeadline(): string {
  return (BigInt(Date.now()) * 1_000_000n + WEEK_NS).toString();
}

const EMPTY = {
  name: "Keyless",
  symbol: "KLT",
  uri: "https://ethics.ltd/k.json",
  target_lamports: "50000000",
  deadline_ns: "",
  dbc_config: "3TPLwDd588cqc28ntKQYWc9gRs7UY7iL7y1FXcLwQ8mj",
  cap_bps: "100",
  fill_fee_bps: "150",
  execution_timeout_ns: "",
  watcher: "keyless-trust-dev-54ef71.testnet",
  curve: JSON.stringify(DEMO_CURVE, null, 2),
};

export default function CreatePage() {
  const [fields, setFields] = useState({ ...EMPTY, deadline_ns: defaultDeadline() });
  const [errors, setErrors] = useState<string[]>([]);
  const [json, setJson] = useState<string | null>(null);
  const [cli, setCli] = useState<string | null>(null);
  const [walletArgs, setWalletArgs] = useState<Record<string, unknown> | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const input = useMemo(
    () => ({
      ...fields,
      fill_fee_bps: fields.fill_fee_bps,
      execution_timeout_ns: fields.execution_timeout_ns,
    }),
    [fields],
  );

  function set<K extends keyof typeof fields>(key: K, value: string) {
    setFields((current) => ({ ...current, [key]: value }));
  }

  function build() {
    setStatus(null);
    const result = validateCreateTrust(input);
    if (!result.ok) {
      setErrors(result.errors);
      setJson(null);
      setCli(null);
      setWalletArgs(null);
      return;
    }
    setErrors([]);
    const text = stringifyNearArgs(result.args);
    setJson(text);
    setCli(buildCreateTrustCli({ contractId: CONTRACT_ID, args: result.args, accountId: "YOUR.testnet" }));
    setWalletArgs(toWalletArgs(result.args) as Record<string, unknown>);
  }

  return (
    <section className="mx-auto w-full max-w-7xl py-8">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Create a Trust</p>
      <h1 className="mt-2 font-display text-4xl font-medium text-ethics-silver">Pin the raise before anyone sends SOL.</h1>
      <p className="mt-3 max-w-2xl text-sm leading-relaxed text-ethics-fog">
        create_trust is permissionless on testnet. It needs at least 0.01 NEAR and about 100 TGas, because the contract derives the raise, fee, and mint keys. cap_bps is required. The curve is stored and later quoted. Parameters do not change after this call.
      </p>

      <form
        className="mt-8 grid gap-4 md:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault();
          build();
        }}
      >
        <Field label="Name" value={fields.name} onChange={(value) => set("name", value)} />
        <Field label="Symbol" value={fields.symbol} onChange={(value) => set("symbol", value)} />
        <Field label="URI (60 characters max)" value={fields.uri} onChange={(value) => set("uri", value)} />
        <Field label="Target lamports" value={fields.target_lamports} onChange={(value) => set("target_lamports", value)} />
        <Field label="Deadline (NEAR nanoseconds)" value={fields.deadline_ns} onChange={(value) => set("deadline_ns", value)} />
        <Field label="DBC config pubkey" value={fields.dbc_config} onChange={(value) => set("dbc_config", value)} />
        <Field label="cap_bps (1 to 10000)" value={fields.cap_bps} onChange={(value) => set("cap_bps", value)} />
        <Field label="fill_fee_bps (blank uses 150)" value={fields.fill_fee_bps} onChange={(value) => set("fill_fee_bps", value)} />
        <Field
          label="Execution timeout ns (optional)"
          value={fields.execution_timeout_ns}
          onChange={(value) => set("execution_timeout_ns", value)}
        />
        <Field label="Watcher account" value={fields.watcher} onChange={(value) => set("watcher", value)} />
        <label className="block text-sm text-ethics-fog md:col-span-2">
          Curve JSON
          <textarea
            className="ethics-input mt-1 w-full"
            rows={14}
            spellCheck={false}
            value={fields.curve}
            onChange={(event) => set("curve", event.target.value)}
          />
        </label>
        <div className="md:col-span-2">
          <button
            type="submit"
            className="inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade"
          >
            Check and build the call
          </button>
        </div>
      </form>

      {errors.length > 0 ? (
        <div className="kt-error mt-4" role="alert">
          <p>The contract would reject this Trust.</p>
          <ul className="mt-2 list-disc pl-5">
            {errors.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {json && cli && walletArgs ? (
        <div className="mt-6 grid gap-4">
          <article className="ethics-panel rounded-2xl p-5">
            <h2 className="font-display text-2xl text-ethics-silver">Arguments</h2>
            <p className="mt-2 text-sm text-ethics-fog">
              The CLI keeps every digit of deadline_ns. A wallet call passes those integers through JavaScript numbers, which rounds a nanosecond timestamp by a few hundred nanoseconds. That does not change a deadline measured in hours.
            </p>
            <pre className="kt-mono mt-4 whitespace-pre-wrap text-ethics-fog">{json}</pre>
            <NearGate
              label="Create with NEAR wallet"
              call={{
                contractId: CONTRACT_ID,
                method: "create_trust",
                args: walletArgs,
                gas: tgas(100),
                deposit: nearToYocto("0.01"),
              }}
              onResult={() => setStatus("create_trust was submitted. The new id is the return value in the wallet. It will show up on the Trust list after the derive callbacks land.")}
              onError={(message) => setErrors([message])}
            />
          </article>
          <article className="ethics-panel rounded-2xl p-5">
            <h2 className="font-display text-2xl text-ethics-silver">near CLI</h2>
            <pre className="kt-mono mt-4 whitespace-pre-wrap text-ethics-fog">{cli}</pre>
          </article>
        </div>
      ) : null}
      {status ? (
        <p className="mt-4 text-sm text-ethics-jade" role="status">
          {status}
        </p>
      ) : null}
    </section>
  );
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const id = label.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return (
    <label className="block text-sm text-ethics-fog" htmlFor={id}>
      {label}
      <input id={id} className="ethics-input mt-1 w-full" value={value} spellCheck={false} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}
