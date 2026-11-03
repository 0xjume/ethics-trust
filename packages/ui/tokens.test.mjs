import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { createRequire } from "node:module";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const theme = createRequire(import.meta.url)("./tailwind.theme.cjs");

describe("Ethics Launch tokens", () => {
  it("defines the core palette", () => {
    for (const token of [
      "--ethics-night: #0b0d0f",
      "--ethics-teal: #38bfa7",
      "--ethics-jade: #63d6b4",
      "--ethics-silver: #d9dee3",
      "--font-display",
      "--radius-lg",
    ]) {
      assert.match(css, new RegExp(token.replace("#", "#")));
    }
  });

  it("exposes the same hex values on the Tailwind theme", () => {
    assert.equal(theme.theme.extend.colors.ethics.teal, "#38BFA7");
    assert.equal(theme.theme.extend.colors.ethics.night, "#0B0D0F");
    assert.ok(theme.theme.extend.fontFamily.display[0].includes("Cormorant"));
  });
});
