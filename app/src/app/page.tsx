import Link from "next/link";

const STEPS = [
  {
    n: "01",
    title: "Back the raise",
    body: "Send SOL to your own derived deposit address. The cap is set at create (1% is 100 bps). Miss the deadline and the contract will only sign a refund to you.",
  },
  {
    n: "02",
    title: "The contract launches it",
    body: "At the target, the watcher asks NEAR to sign one Meteora DBC create-and-buy. The curve, config, and first-buy quote are pinned. No server key can change the destination.",
  },
  {
    n: "03",
    title: "Holders get the tokens",
    body: "Tokens quoted at launch are paid out pro-rata. Rounding dust stays in the raise account. A later stretch would split creator fees 70 / 20 / 10. That split is not in this contract.",
  },
];

export default function HomePage() {
  return (
    <>
      <section className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-0 pt-6 md:pt-8">
          <div className="ethics-panel relative overflow-hidden rounded-2xl px-5 py-8 md:px-10 md:py-10">
            <p className="text-[0.65rem] uppercase tracking-[0.2em] text-ethics-fog/75">
              Real markets. Locked liquidity. No holder.
            </p>
            <p className="mt-6 text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Keyless Trust</p>
            <h1 className="mt-3 max-w-2xl font-display text-3xl font-medium leading-tight tracking-tight text-ethics-silver md:text-4xl">
              Raise a launch together. No one holds the vaults.
            </h1>
            <p className="mt-4 max-w-xl text-sm leading-relaxed text-ethics-fog">
              A NEAR contract owns every Solana vault through Chain Signatures and will only sign allowed transaction shapes: refund, capped sweep, a Meteora DBC create-and-buy at the pinned config, and pro-rata distribution.
            </p>
            <div className="mt-7 flex flex-wrap items-center gap-x-5 gap-y-3">
              <Link
                href="/create"
                className="inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night transition hover:bg-ethics-jade"
              >
                Create a Trust
              </Link>
              <Link
                href="/trusts"
                className="inline-flex items-center rounded-lg border border-ethics-slate px-5 py-2.5 text-sm font-medium text-ethics-silver transition hover:border-ethics-teal/40"
              >
                Discover Trusts
              </Link>
              <a href="#how-it-works" className="text-sm text-ethics-fog transition hover:text-ethics-teal">
                How it works →
              </a>
            </div>
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-7xl py-10" aria-labelledby="flow-heading">
        <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">The mechanic</p>
        <h2 id="flow-heading" className="mt-2 font-display text-2xl font-medium text-ethics-silver">
          Five doors. The contract holds the keys.
        </h2>
        <figure className="ethics-panel mt-6 rounded-2xl p-5">
          <svg viewBox="0 0 920 220" role="img" aria-labelledby="flow-title flow-desc" className="h-auto w-full text-ethics-teal">
            <title id="flow-title">Keyless Trust flow</title>
            <desc id="flow-desc">
              A contributor sends SOL to a deposit address derived by Chain Signatures. The NEAR contract can sign a refund back to that contributor or a capped sweep into the raise vault. From the raise vault it can sign a Meteora DBC create-and-buy, then a pro-rata token distribution. It will not sign any other destination.
            </desc>
            <rect x="16" y="28" width="150" height="64" rx="12" className="fill-ethics-night stroke-ethics-teal" strokeWidth="1.25" />
            <text x="91" y="56" textAnchor="middle" className="fill-ethics-silver" fontSize="13">Contributor</text>
            <text x="91" y="76" textAnchor="middle" className="fill-ethics-fog" fontSize="11">Phantom · devnet</text>
            <path d="M174 60 H230" className="stroke-ethics-fog" strokeWidth="1.25" />
            <path d="M222 54 L234 60 L222 66" className="fill-ethics-fog" />
            <rect x="236" y="28" width="170" height="64" rx="12" className="fill-ethics-slate/80" />
            <text x="321" y="54" textAnchor="middle" className="fill-ethics-silver" fontSize="13">Deposit address</text>
            <text x="321" y="74" textAnchor="middle" className="fill-ethics-fog" fontSize="11">trust/id/in/you</text>
            <path d="M321 100 V132" className="stroke-ethics-fog" strokeWidth="1.25" />
            <path d="M315 124 L321 136 L327 124" className="fill-ethics-fog" />
            <rect x="236" y="140" width="170" height="56" rx="12" className="fill-ethics-night stroke-ethics-silver" strokeWidth="1" />
            <text x="321" y="164" textAnchor="middle" className="fill-ethics-silver" fontSize="13">Refund</text>
            <text x="321" y="182" textAnchor="middle" className="fill-ethics-fog" fontSize="11">only back to you</text>
            <path d="M414 60 H470" className="stroke-ethics-teal" strokeWidth="1.5" />
            <path d="M462 54 L474 60 L462 66" className="fill-ethics-teal" />
            <rect x="476" y="28" width="160" height="64" rx="12" className="fill-ethics-slate/80" />
            <text x="556" y="54" textAnchor="middle" className="fill-ethics-silver" fontSize="13">Raise vault</text>
            <text x="556" y="74" textAnchor="middle" className="fill-ethics-fog" fontSize="11">capped sweep</text>
            <path d="M644 60 H700" className="stroke-ethics-teal" strokeWidth="1.5" />
            <path d="M692 54 L704 60 L692 66" className="fill-ethics-teal" />
            <rect x="706" y="20" width="190" height="80" rx="12" className="fill-ethics-night stroke-ethics-teal" strokeWidth="1.25" />
            <text x="801" y="48" textAnchor="middle" className="fill-ethics-silver" fontSize="13">Meteora DBC</text>
            <text x="801" y="68" textAnchor="middle" className="fill-ethics-fog" fontSize="11">pinned config + curve</text>
            <text x="801" y="86" textAnchor="middle" className="fill-ethics-fog" fontSize="11">pro-rata tokens out</text>
            <text x="16" y="208" className="fill-ethics-fog" fontSize="12">NEAR testnet signs through v1.signer-prod.testnet · domain 1 · Ed25519. The relayer pays gas. It cannot pick a new destination.</text>
          </svg>
          <figcaption className="mt-3 text-sm leading-relaxed text-ethics-fog">
            Allowed shapes: refund to the contributor, a capped sweep into the raise vault, one DBC create-and-buy, and pro-rata distribution of the quoted tokens. Anything else is rejected in the contract.
          </figcaption>
        </figure>
      </section>

      <section id="how-it-works" className="mx-auto w-full max-w-7xl pb-10">
        <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">How it works</p>
        <h2 className="mt-2 font-display text-2xl font-medium text-ethics-silver">Three steps. Keys stay on NEAR.</h2>
        <div className="mt-6 grid gap-4 md:grid-cols-3">
          {STEPS.map((step) => (
            <article key={step.n} className="ethics-panel rounded-2xl p-5">
              <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">{step.n}</p>
              <h3 className="mt-3 font-display text-xl text-ethics-silver">{step.title}</h3>
              <p className="mt-2 text-sm leading-relaxed text-ethics-fog">{step.body}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="mx-auto w-full max-w-7xl pb-4" aria-labelledby="risk-heading">
        <h2 id="risk-heading" className="sr-only">Risk note</h2>
        <div className="kt-note" role="note">
          <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">Honest risk note</p>
          <p className="mt-3">
            This is experimental software on NEAR testnet and Solana devnet only. It moves no mainnet value. A pooled raise where people send money to buy a new token can be a regulated security, unlicensed crowdfunding, or a collective investment in many jurisdictions, including under tests like Howey. Making the vault keyless does not change that. This is not financial, legal, or investment advice. The MPC network and the named watcher are trust assumptions: a watcher can delay a raise or skew the split among registered contributors, and cannot send funds to an address the contract did not fix.
          </p>
        </div>
      </section>
    </>
  );
}
