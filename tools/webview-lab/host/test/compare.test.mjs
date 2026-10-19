// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { buildSummary, compareAll, compareEncoding, compareTree, firstDifference, main, parseResult } from "../compare.mjs";
import { CASES_DIR, MARKER } from "../lab.mjs";
import { loadParse5 } from "../parse5.mjs";
import { serializeParse5 } from "../tree.mjs";

const parse5 = await loadParse5();
const tree = (html, scripting) => serializeParse5(parse5.parse(html, { scriptingEnabled: scripting }));
const ok = (extra) => ({ walker: "ok", tree: null, characterSet: null, probe: null, error: null, ...extra });

test("the parse5 serializer gives the html5lib format for known input", () => {
  assert.equal(
    tree("<!DOCTYPE html><body><noscript><b>x</b></noscript><!--c-->", false),
    ["| <!DOCTYPE html>", "| <html>", "|   <head>", "|   <body>", "|     <noscript>", "|       <b>", '|         "x"', "|     <!-- c -->"].join("\n"),
  );
  assert.ok(tree("<body><noscript><b>x</b></noscript>", true).endsWith('|     <noscript>\n|       "<b>x</b>"'));
});

test("equal trees compare equal", () => {
  const html = "<body><noscript><p>a</p></noscript>";
  const v = compareTree(parse5, Buffer.from(html), "off", ok({ tree: tree(html, false) }));
  assert.deepEqual(v, { status: "equal", detail: "" });
});

test("the scripting view decides which parse5 tree is the reference", () => {
  const html = "<body><noscript><p>a</p></noscript>";
  // The tree of view "on" is not the tree of view "off".
  assert.equal(compareTree(parse5, Buffer.from(html), "off", ok({ tree: tree(html, true) })).status, "different");
  assert.equal(compareTree(parse5, Buffer.from(html), "on", ok({ tree: tree(html, true) })).status, "equal");
});

test("a different tree names the first different line", () => {
  const html = "<body><noscript><p>a</p></noscript>";
  const other = tree(html, true);
  const v = compareTree(parse5, Buffer.from(html), "off", ok({ tree: other }));
  assert.equal(v.status, "different");
  assert.match(v.detail, /^line 5: parse5 "\|       <p>", platform "\|       \\"<p>a<\/p>\\""$/);
});

test("a UTF-8 byte order mark in the body is not part of the text", () => {
  const html = "<body>x";
  const body = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(html)]);
  assert.equal(compareTree(parse5, body, "on", ok({ tree: tree(html, true) })).status, "equal");
});

test("an unavailable walker gives unavailable with the reason", () => {
  const v = compareTree(parse5, Buffer.from("<p>"), "off", { walker: "unavailable", tree: null, characterSet: null, probe: null, error: "no JS in view off" });
  assert.deepEqual(v, { status: "unavailable", detail: "no JS in view off" });
});

test("firstDifference: equal, shorter and longer trees", () => {
  assert.equal(firstDifference("a\nb", "a\nb"), null);
  assert.equal(firstDifference("a\nb", "a"), 'line 2: parse5 "b", platform (end of tree)');
  assert.equal(firstDifference("a", "a\nb"), 'line 2: parse5 (end of tree), platform "b"');
});

test("Q-20 utf8 expectation: UTF-8 and the marker text", () => {
  assert.equal(compareEncoding("utf8", ok({ characterSet: "UTF-8", probe: MARKER })).status, "equal");
  assert.equal(compareEncoding("utf8", ok({ characterSet: "windows-1252", probe: MARKER })).status, "different");
  assert.equal(compareEncoding("utf8", ok({ characterSet: "UTF-8", probe: "Gr\u00c3\u00bc" })).status, "different");
  assert.equal(compareEncoding("utf8", ok({ characterSet: null, probe: null })).status, "different");
});

test("Q-20 latin1 expectation: the UTF-8 bytes read as windows-1252", () => {
  const mojibake = new TextDecoder("windows-1252").decode(new TextEncoder().encode(MARKER));
  assert.ok(mojibake.startsWith("GrÃ¼ÃŸe"));
  assert.equal(compareEncoding("latin1", ok({ characterSet: "windows-1252", probe: mojibake })).status, "equal");
  assert.equal(compareEncoding("latin1", ok({ characterSet: "ISO-8859-1", probe: mojibake })).status, "equal");
  assert.equal(compareEncoding("latin1", ok({ characterSet: "UTF-8", probe: MARKER })).status, "different");
});

test("Q-20 info expectation records what the WebView did and never differs", () => {
  const v = compareEncoding("info", ok({ characterSet: "windows-1252", probe: "x" }));
  assert.equal(v.status, "info");
  assert.match(v.detail, /windows-1252/);
});

test("Q-20 with an unavailable walker is unavailable", () => {
  assert.equal(compareEncoding("utf8", { walker: "unavailable", tree: null, characterSet: null, probe: null, error: "r" }).status, "unavailable");
});

test("parseResult rejects a malformed result file", () => {
  const good = { id: "A", view: "on", walker: "ok", tree: "| <html>", characterSet: "UTF-8", contentType: "text/html", probe: null, error: null };
  assert.equal(parseResult(JSON.stringify(good), "A", "on", "f").walker, "ok");
  const bad = (o, id = "A", view = "on") => () => parseResult(typeof o === "string" ? o : JSON.stringify(o), id, view, "f");
  const code = { code: "RESULT_MALFORMED" };
  assert.throws(bad("{"), code);
  assert.throws(bad("[]"), code);
  assert.throws(bad("null"), code);
  assert.throws(bad(good, "B"), code);
  assert.throws(bad(good, "A", "off"), code);
  assert.throws(bad({ ...good, walker: "maybe" }), code);
  assert.throws(bad({ ...good, tree: 5 }), code);
  assert.throws(bad({ ...good, tree: null }), code);
  assert.throws(bad({ ...good, error: {} }), code);
  assert.throws(bad({ ...good, probe: 1 }), code);
  assert.throws(bad({ ...good, characterSet: [] }), code);
});

test("parseResult accepts an unavailable walker without a tree", () => {
  const r = parseResult(JSON.stringify({ id: "A", view: "off", walker: "unavailable", tree: null, error: "why" }), "A", "off", "f");
  assert.equal(r.walker, "unavailable");
  assert.equal(r.error, "why");
});

test("the summary has one row per case and view and escapes table characters", () => {
  const md = buildSummary("ios", [
    { id: "A", view: "on", question: "Q-19", verdict: { status: "equal", detail: "" } },
    { id: "B", view: "off", question: "Q-19", verdict: { status: "different", detail: 'a | b\nc' } },
  ]);
  assert.match(md, /Rows: 2\. Equal: 1\. Different: 1\. Info: 0\. Unavailable: 0\. Missing: 0\./);
  assert.match(md, /\| A \| Q-19 \| on \| equal \|  \|/);
  assert.match(md, /\| B \| Q-19 \| off \| different \| a \\\| b c \|/);
});

// A results directory for a few real cases.
let dir;
before(() => {
  dir = mkdtempSync(join(tmpdir(), "lab-results-"));
});
after(() => rmSync(dir, { recursive: true, force: true }));

function resultsFor(name) {
  const d = join(dir, name);
  mkdirSync(d, { recursive: true });
  return d;
}

test("compareAll over the real case list: parse5 as the platform is all equal; absent files are missing", async () => {
  const d = resultsFor("self");
  const ids = JSON.parse(readFileSync(join(CASES_DIR, "index.json"), "utf8"));
  const written = [];
  for (const id of ids.slice(0, 6)) {
    const c = JSON.parse(readFileSync(join(CASES_DIR, `${id}.json`), "utf8"));
    const html = new TextDecoder().decode(readFileSync(join(CASES_DIR, "..", "..", "..", c.source)));
    for (const view of c.views) {
      writeFileSync(join(d, `${id}.${view}.json`), JSON.stringify({ id, view, walker: "ok", tree: tree(html, view === "on"), characterSet: "UTF-8", probe: null, error: null }));
      written.push(`${id}.${view}`);
    }
  }
  const { rows, summary } = await compareAll({ platform: "ios", resultsDir: d, parse5 });
  const byStatus = (s) => rows.filter((r) => r.verdict.status === s).length;
  assert.equal(byStatus("equal"), written.length);
  assert.equal(byStatus("missing"), rows.length - written.length);
  assert.equal(byStatus("different"), 0);
  assert.ok(summary.includes("| CT-UTF8 | Q-20 | on | missing |"));
});

test("main exits 0 when platform and parse5 differ, and writes summary.md", async () => {
  const d = resultsFor("differs");
  writeFileSync(join(d, "NOSCRIPT.on.json"), JSON.stringify({ id: "NOSCRIPT", view: "on", walker: "ok", tree: "| <html>", error: null }));
  assert.equal(await main(["--platform", "ios", "--results", d]), 0);
  assert.match(readFileSync(join(d, "summary.md"), "utf8"), /\| NOSCRIPT \| Q-19 \| on \| different \| line 1:/);
});

test("main exits 1 for a malformed result file", async () => {
  const d = resultsFor("malformed");
  writeFileSync(join(d, "NOSCRIPT.on.json"), "not json");
  assert.equal(await main(["--platform", "android", "--results", d]), 1);
});

test("main exits 2 for a bad platform or a missing results directory", async () => {
  assert.equal(await main(["--platform", "web"]), 2);
  assert.equal(await main([]), 2);
  assert.equal(await main(["--platform", "ios", "--results", join(dir, "nope")]), 2);
});
