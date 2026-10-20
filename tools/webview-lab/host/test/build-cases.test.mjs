// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { diffCases, expectedFiles, writeCases } from "../build-cases.mjs";
import { CASES_DIR, Q20_CASES, parseCase } from "../lab.mjs";

test("the committed cases/ is current", async () => {
  assert.deepEqual(await diffCases(CASES_DIR, await expectedFiles()), []);
});

test("output is deterministic", async () => {
  const a = await expectedFiles();
  const b = await expectedFiles();
  assert.deepEqual([...a], [...b]);
});

test("the case list has the Q-19 fixtures and the 8 Q-20 cases, all valid", async () => {
  const files = await expectedFiles();
  const ids = JSON.parse(files.get("index.json"));
  const cases = ids.map((id) => parseCase(JSON.parse(files.get(`${id}.json`)), id));
  const q20 = cases.filter((c) => c.question === "Q-20");
  assert.deepEqual(q20.map((c) => c.id), Q20_CASES.map((q) => q.id));
  assert.equal(q20.length, 8);
  assert.ok(q20.every((c) => c.views.join() === "on" && c.probe === "#m"));
  const q19 = cases.filter((c) => c.question === "Q-19");
  assert.ok(q19.length >= 40);
  assert.ok(q19.every((c) => c.views.join() === "on,off" && c.headers.length === 1 && c.headers[0][1] === "text/html; charset=utf-8"));
  assert.ok(q19.some((c) => c.id === "EDGE-E3-NOSCRIPT-EOF"));
  assert.ok(q19.some((c) => c.id === "NOSCRIPT-SVG" && c.source === "fixtures/rqp/markup/FX-RQ-MARKUP-NOSCRIPT-SVG/input.html"));
  assert.ok(q19.every((c) => /NOSCRIPT|^EDGE-E[38]-/.test(c.id)));
});

test("Q-20 cases have the headers of the contract", async () => {
  const files = await expectedFiles();
  const headers = (id) => JSON.parse(files.get(`${id}.json`)).headers;
  assert.deepEqual(headers("CT-NONE"), []);
  assert.deepEqual(headers("CT-NO-CHARSET"), [["Content-Type", "text/html"]]);
  assert.equal(headers("CT-TWO-LINES").length, 2);
  assert.equal(JSON.parse(files.get("BOM-NONE.json")).source, "tools/webview-lab/host/pages/q20-bom.html");
});

test("the BOM page starts with a byte order mark and the other pages do not", () => {
  const pages = join(CASES_DIR, "..", "host", "pages");
  assert.deepEqual([...readFileSync(join(pages, "q20-bom.html")).subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  for (const name of ["q20.html", "q20-meta-latin1.html"]) {
    assert.notDeepEqual([...readFileSync(join(pages, name)).subarray(0, 3)], [0xef, 0xbb, 0xbf]);
    assert.ok(readFileSync(join(pages, name), "utf8").includes('<p id="m">Grüße – 日本</p>'));
  }
});

test("--check logic finds a changed, a missing and a stale file; write repairs all", async () => {
  const dir = mkdtempSync(join(tmpdir(), "lab-cases-"));
  try {
    const files = await expectedFiles();
    await writeCases(dir, files);
    assert.deepEqual(await diffCases(dir, files), []);
    writeFileSync(join(dir, "CT-UTF8.json"), "{}\n");
    writeFileSync(join(dir, "OLD.json"), "{}\n");
    rmSync(join(dir, "BOM-NONE.json"));
    assert.deepEqual(await diffCases(dir, files), ["BOM-NONE.json: missing", "CT-UTF8.json: changed", "OLD.json: stale"]);
    await writeCases(dir, files);
    assert.deepEqual(await diffCases(dir, files), []);
    assert.equal(readdirSync(dir).length, files.size);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a missing cases/ directory is reported, not crashed on", async () => {
  const diffs = await diffCases(join(tmpdir(), "lab-no-such-dir"), new Map([["A.json", "x"]]));
  assert.deepEqual(diffs, ["A.json: missing"]);
});
