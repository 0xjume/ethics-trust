import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { buildQuoteFixtures } from "./generate-quote-fixtures.ts";

describe("DBC quote fixtures", () => {
  it("matches the SDK 2.0.0 quotes committed for the Rust port", async () => {
    const path = join(
      dirname(fileURLToPath(import.meta.url)),
      "..",
      "..",
      "contracts",
      "keyless-trust",
      "tests",
      "fixtures",
      "dbc_quotes.json",
    );
    const committed = JSON.parse(readFileSync(path, "utf8"));
    const fresh = await buildQuoteFixtures();
    assert.deepEqual(fresh, committed);
  });
});
