import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { startHealthServer } from "./health.ts";

describe("health", () => {
  it("answers GET /health with 200", async () => {
    const server = await startHealthServer(0);
    const address = server.address();
    const port = address && typeof address === "object" ? address.port : 0;
    try {
      const ok = await fetch(`http://127.0.0.1:${port}/health`);
      assert.equal(ok.status, 200);
      const body = (await ok.json()) as { ok: boolean };
      assert.equal(body.ok, true);
      const missing = await fetch(`http://127.0.0.1:${port}/other`);
      assert.equal(missing.status, 404);
    } finally {
      await new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    }
  });
});
