export function logEvent(fields: Record<string, unknown>): void {
  const out: Record<string, unknown> = {
    ts: new Date().toISOString(),
    service: "keyless-trust-relayer",
  };
  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined) continue;
    out[key] = typeof value === "bigint" ? value.toString() : value;
  }
  console.log(JSON.stringify(out));
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
