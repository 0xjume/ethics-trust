"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";
import { ConnectBar } from "./ConnectBar";

const LINKS = [
  { href: "/", label: "Home" },
  { href: "/trusts", label: "Trusts" },
  { href: "/create", label: "Create" },
  { href: "/verify", label: "Verify" },
];

function Spark({ className }: { className: string }) {
  return (
    <svg className={className} viewBox="0 0 28 28" fill="currentColor" aria-hidden="true">
      <path d="M14 0 L16.2 11.8 L28 14 L16.2 16.2 L14 28 L11.8 16.2 L0 14 L11.8 11.8 Z" />
    </svg>
  );
}

export function Shell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const active = (href: string) => (href === "/" ? pathname === "/" : pathname.startsWith(href));

  return (
    <div className="flex min-h-screen flex-col ethics-grid-bg text-ethics-silver font-sans">
      <a
        href="#content"
        className="sr-only kt-skip"
      >
        Skip to content
      </a>
      <header className="sticky top-0 z-40 w-full border-b border-ethics-slate/60 bg-ethics-night/80 backdrop-blur-md">
        <div className="mx-auto flex max-w-7xl items-center justify-between gap-4 px-4 py-3 md:px-6">
          <Link href="/" className="inline-flex hover:opacity-90 transition">
            <span className="inline-flex items-baseline gap-1.5">
              <span className="font-display text-xl font-medium tracking-tight text-ethics-silver">Keyless Trust</span>
              <Spark className="kt-spark h-2.5 w-2.5 shrink-0 text-ethics-teal" />
            </span>
          </Link>
          <nav className="hidden items-center gap-6 md:flex" aria-label="Primary">
            {LINKS.map((link) => (
              <Link
                key={link.href}
                href={link.href}
                className={`text-sm tracking-wide transition ${active(link.href) ? "text-ethics-teal" : "text-ethics-fog hover:text-ethics-silver"}`}
                aria-current={active(link.href) ? "page" : undefined}
              >
                {link.label}
              </Link>
            ))}
          </nav>
          <div className="flex items-center gap-2 md:gap-3">
            <Link
              href="/create"
              className="inline-flex items-center rounded-lg bg-ethics-teal px-3 py-2 text-xs font-medium text-ethics-night hover:bg-ethics-jade md:px-4 md:text-sm"
            >
              Launch
            </Link>
            <ConnectBar />
          </div>
        </div>
        <nav className="border-t border-ethics-slate/40 md:hidden" aria-label="Mobile">
          <div className="flex items-center gap-4 overflow-x-auto px-4 py-2">
            {LINKS.map((link) => (
              <Link
                key={link.href}
                href={link.href}
                className={`shrink-0 text-xs tracking-wide ${active(link.href) ? "text-ethics-teal" : "text-ethics-fog"}`}
              >
                {link.label}
              </Link>
            ))}
          </div>
        </nav>
      </header>
      <div id="content" className="flex flex-1 flex-col items-center px-4 pb-16 pt-0 md:px-6">
        <div className="w-full lg:max-w-7xl">{children}</div>
      </div>
      <footer className="relative mt-auto overflow-hidden border-t border-ethics-slate/60 bg-ethics-night/90">
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-ethics-teal/25 to-transparent" aria-hidden="true" />
        <div className="relative z-10 mx-auto grid max-w-7xl gap-10 px-4 py-12 md:grid-cols-3 md:px-6 md:py-16">
          <div>
            <span className="inline-flex items-baseline gap-1.5">
              <span className="font-display text-xl font-medium tracking-tight">Keyless Trust</span>
              <Spark className="kt-spark h-2.5 w-2.5 shrink-0 text-ethics-teal" />
            </span>
            <p className="mt-4 max-w-sm text-sm leading-relaxed text-ethics-fog">
              Community raises whose Solana vaults no person or server holds. A NEAR contract signs only the shapes it was built to allow.
            </p>
            <p className="mt-4 text-sm text-ethics-fog">
              Keyless Trust · by{" "}
              <a className="text-ethics-teal hover:text-ethics-jade" href="https://ethics.ltd" rel="noreferrer">
                Ethics
              </a>
            </p>
          </div>
          <div>
            <p className="text-[0.65rem] uppercase tracking-[0.22em] text-ethics-fog/70">Product</p>
            <ul className="mt-4 space-y-3">
              {LINKS.map((link) => (
                <li key={link.href}>
                  <Link className="text-sm text-ethics-fog transition hover:text-ethics-silver" href={link.href}>
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <p className="text-[0.65rem] uppercase tracking-[0.22em] text-ethics-fog/70">Networks</p>
            <ul className="mt-4 space-y-3 text-sm text-ethics-fog">
              <li>NEAR testnet</li>
              <li>Solana devnet</li>
              <li>Chain Signatures domain 1</li>
            </ul>
          </div>
        </div>
        <div className="relative z-10 mx-auto flex max-w-7xl flex-col gap-3 border-t border-ethics-slate/50 px-4 py-6 sm:flex-row sm:items-center sm:justify-between md:px-6">
          <p className="text-xs text-ethics-fog/70">NEAR HACKATHON 2026 · memecoins · testnet and devnet only.</p>
          <p className="text-xs text-ethics-fog/60">Not investment advice. Experimental software.</p>
        </div>
      </footer>
    </div>
  );
}
