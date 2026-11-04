import Link from "next/link";

export default function NotFound() {
  return (
    <section className="mx-auto w-full max-w-7xl py-10">
      <p className="text-[0.65rem] uppercase tracking-[0.18em] text-ethics-teal">404</p>
      <h1 className="mt-3 font-display text-4xl text-ethics-silver">That page is not in Keyless Trust.</h1>
      <p className="mt-3 text-sm text-ethics-fog">The contract and the Trust list are still available.</p>
      <Link
        href="/trusts"
        className="mt-6 inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade"
      >
        Discover Trusts
      </Link>
    </section>
  );
}
