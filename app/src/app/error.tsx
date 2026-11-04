"use client";

export default function AppError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <section className="mx-auto w-full max-w-7xl py-10">
      <div className="kt-error" role="alert">
        <p className="font-display text-2xl text-ethics-silver">This page failed to render.</p>
        <p className="mt-2 text-sm text-ethics-fog">{error.message || "Unknown error"}</p>
        <button
          type="button"
          onClick={reset}
          className="mt-4 inline-flex items-center rounded-lg bg-ethics-teal px-5 py-2.5 text-sm font-medium text-ethics-night hover:bg-ethics-jade"
        >
          Try again
        </button>
      </div>
    </section>
  );
}
