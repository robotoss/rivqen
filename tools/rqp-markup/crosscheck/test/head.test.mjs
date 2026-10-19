// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import { REPO_ROOT, main, parseArgs } from "../src/cli.mjs";
import { boundaries, checkHostile, checkItem, clientC1, fullDocumentAccepted, generatedItems, prepare, staticItems } from "../src/head.mjs";
import { generateHeadDocument } from "../src/head-gen.mjs";
import { CrosscheckError } from "../src/run.mjs";
import { parse5View } from "../src/view.mjs";

const SUPPORT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "test-support");
const TMP = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-head-test-"));
after(() => rmSync(TMP, { recursive: true, force: true }));

const buf = (s) => Buffer.from(s, "utf8");
const EXPLICIT = buf('<!DOCTYPE html><html><head>\n<title>t</title></head><body><div data-rq-block="a">x</div></body></html>');
const IMPLIED = buf('<!DOCTYPE html><meta charset="utf-8"><title>t</title><body><div data-rq-block="a">x</div></body>');
const NO_HEAD = buf('<!DOCTYPE html><body><div data-rq-block="a">x</div></body>');

/** The blocks of O in the shape of a candidate result. */
function blocksOf(original) {
  return parse5View(original).blocks.map((b) => ({ id: b.id, format: b.tag === "script" ? "json" : "html", start: b.start, end: b.end }));
}

function check(original, { blocks = blocksOf(original), position, clientPosition } = {}) {
  const item = { name: "t", original };
  const prep = position === undefined ? prepare(item) : prepare(item, position);
  return checkItem(item, prep, blocks, clientPosition === undefined ? {} : { clientPosition });
}

async function run(argv) {
  const lines = [];
  const code = await main(argv, { write: (l) => lines.push(l) });
  return { code, out: lines.join("\n") };
}

function candidatesFile(name, modes) {
  const file = path.join(TMP, `${name}.json`);
  const list = modes.map((m) => ({ name: `fake-${m}`, cwd: SUPPORT, build: null, run: ["node", "fake-candidate.mjs", m, "--batch", "{dir}"] }));
  writeFileSync(file, JSON.stringify(list));
  return file;
}

test("parseArgs: the head command checks both scripting views by default", () => {
  assert.deepEqual(parseArgs(["head"]).views, ["enabled", "disabled"]);
  assert.deepEqual(parseArgs(["head", "--scripting", "enabled"]).views, ["enabled"]);
  assert.throws(() => parseArgs(["head", "--scripting", "spec"]), CrosscheckError);
  assert.throws(() => parseArgs(["head", "--scripting", "all"]), CrosscheckError);
});

test("prepare: the manifest goes after the head start tag, or before the tag that implies the head", () => {
  const e = prepare({ original: EXPLICIT });
  assert.equal(e.at, EXPLICIT.indexOf("<head>") + "<head>".length);
  const i = prepare({ original: IMPLIED });
  assert.equal(i.at, IMPLIED.indexOf("<meta"));
  assert.equal(i.full.subarray(i.at, i.at + 7).toString(), "<script");
  assert.equal(prepare({ original: NO_HEAD }).at, -1);
  assert.equal(prepare({ original: NO_HEAD }).full, null);
});

test("checkItem: a manifest at the shipped point gives no finding (explicit and implied head)", () => {
  assert.deepEqual(check(EXPLICIT), []);
  assert.deepEqual(check(IMPLIED), []);
  assert.deepEqual(check(NO_HEAD), []);
});

test("checkItem control: a manifest before the DOCTYPE changes the tree of the page", () => {
  const kinds = check(EXPLICIT, { position: () => 0, clientPosition: () => 0 }).map((f) => f.kind);
  assert.ok(kinds.includes("parse5-enabled-same"), kinds.join());
  assert.ok(kinds.includes("parse5-disabled-same"), kinds.join());
});

test("checkItem control: a manifest after other head content is not the first child", () => {
  const late = (b) => b.indexOf("<title>");
  const kinds = check(IMPLIED, { position: late, clientPosition: late }).map((f) => f.kind);
  assert.ok(kinds.includes("parse5-enabled-first"), kinds.join());
});

test("checkItem control: a server position that the client does not compute is a C2 finding", () => {
  const kinds = check(EXPLICIT, { position: (b) => b.indexOf("<title>") }).map((f) => f.kind);
  assert.ok(kinds.includes("client-c2"), kinds.join());
});

test("checkItem control: an element that changes the tree of the page (inside a tag) is a finding", () => {
  const original = buf('<!DOCTYPE html><html lang="en"><head></head><body></body></html>');
  const inside = (b) => b.indexOf(' lang="en"');
  const kinds = check(original, { position: inside, clientPosition: inside, blocks: [] }).map((f) => f.kind);
  assert.ok(kinds.length > 0, "a finding is expected");
});

test("checkItem: block boundaries are the candidate blocks shifted by the element", () => {
  const good = blocksOf(EXPLICIT);
  assert.deepEqual(check(EXPLICIT, { blocks: good }), []);
  const moved = good.map((b) => ({ ...b, end: b.end + 1 }));
  assert.deepEqual(check(EXPLICIT, { blocks: moved }).map((f) => f.kind), ["blocks-enabled", "blocks-disabled"]);
  assert.deepEqual(check(EXPLICIT, { blocks: [] }).map((f) => f.kind), ["blocks-enabled", "blocks-disabled"]);
});

test("checkItem: a page that is not valid gets the C2 check but no tree or block check", () => {
  assert.deepEqual(check(EXPLICIT, { blocks: null }), []);
  const kinds = check(EXPLICIT, { blocks: null, position: (b) => b.indexOf("<title>") }).map((f) => f.kind);
  assert.deepEqual(kinds, ["client-c2"]);
});

test("checkItem: an original with a manifest-like element is skipped when not valid, a finding otherwise", () => {
  const withManifest = buf('<!DOCTYPE html><head><script type="application/rivqen-manifest+json">{}</script></head>');
  assert.deepEqual(check(withManifest, { blocks: null }), []);
  assert.deepEqual(check(withManifest, { blocks: [] }).map((f) => f.kind), ["client-elements"]);
});

test("fullDocumentAccepted: a full document counts only when the element is at the server position", () => {
  const full = staticItems(REPO_ROOT).filter((i) => i.full !== undefined);
  assert.ok(full.some((i) => fullDocumentAccepted(i)));
  assert.ok(full.some((i) => !fullDocumentAccepted(i)));
  const accepted = full.find((i) => fullDocumentAccepted(i));
  assert.deepEqual(check(accepted.original, { blocks: null }), []);
  assert.equal(prepare(accepted).full, accepted.full);
});

test("staticItems: fixtures, protocol pages, bodies, examples and cases", () => {
  const names = staticItems(REPO_ROOT).map((i) => i.name);
  for (const prefix of ["fixture:", "page:", "body:", "spec:", "case:"]) assert.ok(names.some((n) => n.startsWith(prefix)), prefix);
  assert.equal(new Set(names).size, names.length);
});

test("generatedItems: the same seed gives the same documents", () => {
  const a = [...generatedItems(5, 0, 20)].map((i) => i.original.toString());
  const b = [...generatedItems(5, 0, 20)].map((i) => i.original.toString());
  assert.deepEqual(a, b);
  assert.equal(a[7], generateHeadDocument(5, 7));
  assert.notEqual(generateHeadDocument(5, 1), generateHeadDocument(6, 1));
  assert.equal([...generatedItems(5, 10, 13)].length, 3);
  assert.equal([...generatedItems(5, 10, 13)][0].name, "gen seed=5 index=10");
});

test("boundaries: token boundaries up to the end of item 1", () => {
  assert.deepEqual(boundaries(buf("<!--c--><head>")), [0, 8, 14]);
});

test("checkHostile: the client rule C2 accepts the server position and no other", () => {
  const original = buf("<!--c--><html><head></head><body></body></html>");
  const r = checkHostile({ original }, prepare({ original }).at);
  assert.equal(r.accepted, 1);
  assert.ok(r.positions >= 4);
  assert.deepEqual(r.findings, []);
});

test("checkHostile control: a walk on F (rule C1) accepts a position that the server does not give", () => {
  const original = buf("<!--c--><head></head><body></body>");
  const r = checkHostile({ original }, prepare({ original }).at, { clientAccepts: (f, s, e) => clientC1(f, s, e) });
  assert.ok(r.findings.some((f) => f.kind === "hostile-position"));
});

test("head command: a small run with a candidate that agrees with parse5 has no finding", async () => {
  const candidates = candidatesFile("agree", ["p5"]);
  const { code, out } = await run(["head", "--candidates", candidates, "--count", "300", "--chunk", "100", "--seed", "3", "--scripting", "enabled", "--out", path.join(TMP, "o1")]);
  assert.equal(code, 0, out);
  assert.match(out, /^head: \d+ documents, 0 findings$/m);
  assert.ok(!out.includes("<"), "the output must not contain document text");
});

test("head command: the number of documents is the fixed inputs plus --count", async () => {
  const candidates = candidatesFile("count", ["invalid"]);
  const a = await run(["head", "--candidates", candidates, "--count", "10", "--out", path.join(TMP, "o2")]);
  const b = await run(["head", "--candidates", candidates, "--count", "30", "--out", path.join(TMP, "o2")]);
  const n = (s) => Number(/head: (\d+) documents/.exec(s)[1]);
  assert.equal(n(b.out) - n(a.out), 20);
  assert.equal(a.code, 0, a.out);
});

test("head command control: a candidate that gives no blocks on pages with blocks is a finding, exit 1", async () => {
  const candidates = candidatesFile("zero", ["valid0"]);
  const r = await run(["head", "--candidates", candidates, "--count", "200", "--chunk", "100", "--out", path.join(TMP, "o3"), "--keep", "2"]);
  assert.equal(r.code, 1);
  assert.match(r.out, /blocks-enabled/);
  assert.match(r.out, /candidate-accepts-f/);
  assert.match(r.out, /^head: \d+ documents, [1-9]\d* findings$/m);
  assert.ok(!r.out.includes("<"), "the output must not contain document text");
});

test("head command: a candidate that fails to run gives exit 1", async () => {
  const candidates = candidatesFile("exit", ["exit"]);
  const r = await run(["head", "--candidates", candidates, "--count", "5", "--out", path.join(TMP, "o4")]);
  assert.equal(r.code, 1);
  assert.match(r.out, /did not run/);
});

test("head command: usage errors give exit 2", async () => {
  assert.equal((await run(["head", "--scripting", "spec"])).code, 2);
  assert.equal((await run(["head", "--only", "nope"])).code, 2);
});
