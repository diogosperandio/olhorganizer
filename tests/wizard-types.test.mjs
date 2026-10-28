import assert from "node:assert/strict";
import test from "node:test";

import {
  customMarkersFromFilter,
  filterWithTypes,
  typesFromFilter,
} from "../src/lib/components/wizard/types.ts";
import { toPlainJson } from "../src/lib/ipc.ts";

test("builds a dated PDF filter from clicks", () => {
  assert.equal(filterWithTypes("{YYYY}{MM}{DD}.*", ["pdf"]), "{YYYY}{MM}{DD}.pdf");
});

test("keeps a valid extension list for images", () => {
  assert.equal(filterWithTypes("*", ["jpg", "png"]), "*.{jpg,png}");
  assert.deepEqual(typesFromFilter("*.{jpg,png}"), ["jpg", "png"]);
});

test("finds custom markers for destination buttons", () => {
  assert.deepEqual(customMarkersFromFilter("{TEXTO:cliente}_{NUM:lote}.{jpg,png}"), [
    { kind: "TEXTO", name: "cliente", token: "{cliente}" },
    { kind: "NUM", name: "lote", token: "{lote}" },
  ]);
});

test("converts reactive proxies into cloneable IPC payloads", () => {
  const reactiveRule = new Proxy({
    destination: "/home/diogo/Downloads/NEWS/{YYYY}/{MM}",
    filter: "{YYYY}{MM}{DD}.pdf",
  }, {});
  const reactiveArgs = new Proxy({ rule: reactiveRule, sampleFileName: "20261022.pdf" }, {});

  assert.throws(() => structuredClone(reactiveArgs), { name: "DataCloneError" });

  const plainArgs = toPlainJson(reactiveArgs);
  assert.doesNotThrow(() => structuredClone(plainArgs));
  assert.deepEqual(plainArgs, {
    rule: {
      destination: "/home/diogo/Downloads/NEWS/{YYYY}/{MM}",
      filter: "{YYYY}{MM}{DD}.pdf",
    },
    sampleFileName: "20261022.pdf",
  });
});
