import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { StatusChip } from "./StatusChip.tsx";

describe("StatusChip", () => {
  it("renders each contract state as text, not color alone", () => {
    for (const state of ["Raising", "Executing", "Launched", "Distributed", "Refunding"]) {
      const html = renderToStaticMarkup(createElement(StatusChip, { state }));
      assert.match(html, new RegExp(state));
      assert.match(html, /rounded-full/);
    }
  });

  it("labels an unexpected state instead of dropping it", () => {
    const html = renderToStaticMarkup(createElement(StatusChip, { state: "Paused" }));
    assert.match(html, /Unknown/);
  });
});
