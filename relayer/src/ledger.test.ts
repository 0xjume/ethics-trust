import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { decodePending, ledgerKeysFromState } from "./ledger.ts";

describe("ledger state", () => {
  it("reads contributor keys from the cv index", () => {
    const value = Buffer.from(
      Uint8Array.from([
        0x2e,
        0,
        0,
        0,
        ...Buffer.from("1:CheLEbm6ELdLHSqju5K5srLGBWeFUMStLWFbDhd3nbbn"),
      ]),
    ).toString("base64");
    const rows = ledgerKeysFromState([{ value }]);
    assert.equal(rows.length, 1);
    assert.equal(rows[0]?.trustId, "1");
    assert.equal(rows[0]?.contributor, "CheLEbm6ELdLHSqju5K5srLGBWeFUMStLWFbDhd3nbbn");
  });

  it("decodes a live pending launch row", () => {
    const hex =
      "050000000000000000000000060000006c61756e6368e05bb4000000000040000000366332336361616138376430363733646563393230336539323361623232356638376164336461623063353364613637613431656437373138383636323738340b5c466619b5e4180000010000000000000000330000004270675465573551746f725075665632775739744643707143344d6a6e50614c5239325579714e7a4e6648313a313830303030";
    const row = decodePending(Buffer.from(hex, "hex"));
    assert.ok(row);
    assert.equal(row.trustId, "5");
    assert.equal(row.contributor, "");
    assert.equal(row.kind, "launch");
    assert.equal(row.lamports, 11_820_000n);
    assert.equal(
      row.messageHash,
      "6c23caaa87d0673dec9203e923ab225f87ad3dab0c53da67a41ed77188662784",
    );
    assert.equal(row.settled, false);
    assert.equal(row.released, false);
    assert.equal(row.superseded, false);
  });

  it("returns null for a truncated value", () => {
    assert.equal(decodePending(Uint8Array.from([1, 2, 3])), null);
  });
});
