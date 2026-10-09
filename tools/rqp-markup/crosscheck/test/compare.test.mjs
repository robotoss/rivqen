// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { compareCandidates, compareWithView, resultShapeError, show } from "../src/compare.mjs";

const view = (blocks, manifests = 0) => ({ ok: true, blocks, manifests });
const p5 = (id, start, end, extra = {}) => ({ id, tag: "div", html: true, start, end, type: null, ...extra });
const valid = (blocks) => ({ valid: true, error: null, blocks });
const blk = (id, start, end, format = "html") => ({ id, format, start, end, sha256: "x" });

test("equal blocks agree", () => {
  const c = compareWithView(valid([blk("a", 5, 9)]), view([p5("a", 5, 9)]));
  assert.deepEqual(c, { status: "agree", details: [], categories: [] });
});

test("an invalid result is not checked", () => {
  const c = compareWithView({ valid: false, error: "RQP_MARKUP_STRUCTURE", blocks: [] }, view([p5("a", 1, 2)]));
  assert.equal(c.status, "rejected");
});

test("a different end offset disagrees", () => {
  const c = compareWithView(valid([blk("a", 5, 9)]), view([p5("a", 5, 7)]));
  assert.equal(c.status, "disagree");
  assert.deepEqual(c.categories, ["blocks"]);
  assert.deepEqual(c.details, ['block 0 ("a") end: candidate 9, parse5 7']);
});

test("an implied end in parse5 disagrees", () => {
  const c = compareWithView(valid([blk("a", 5, 9)]), view([p5("a", 5, null)]));
  assert.match(c.details[0], /parse5 has no end tag/);
});

test("start, id, namespace and format differences are reported", () => {
  const c = compareWithView(valid([blk("a", 5, 9, "json")]), view([p5("b", 4, 9, { html: false })]));
  assert.equal(c.details.length, 4);
  assert.ok(c.details.some((d) => d.includes("start: candidate 5, parse5 4")));
  assert.ok(c.details.some((d) => d.includes("not in the HTML namespace")));
  assert.ok(c.details.some((d) => d.includes('id: parse5 "b"')));
  assert.ok(c.details.some((d) => d.includes("format")));
});

test("a json block must be a script element in parse5", () => {
  const ok = compareWithView(valid([blk("j", 5, 9, "json")]), view([p5("j", 5, 9, { tag: "script" })]));
  assert.equal(ok.status, "agree");
});

test("an extra parse5 block and a missing parse5 block disagree", () => {
  const extra = compareWithView(valid([]), view([p5("x", 1, 2)]));
  assert.deepEqual(extra.details, ["block count: candidate 0, parse5 1", 'parse5 block 0 ("x") at 1 is not in the candidate result']);
  const missing = compareWithView(valid([blk("a", 1, 2)]), view([]));
  assert.deepEqual(missing.details, ["block count: candidate 1, parse5 0"]);
});

test("a manifest-like script in the tree disagrees with a valid result", () => {
  const c = compareWithView(valid([]), view([], 1));
  assert.deepEqual(c.categories, ["manifest"]);
});

test("a valid result for a document without a tree disagrees", () => {
  const c = compareWithView(valid([]), { ok: false, reason: "input is not valid UTF-8", blocks: [], manifests: 0 });
  assert.deepEqual([c.status, c.categories], ["disagree", ["no-tree"]]);
});

test("result shapes are checked", () => {
  assert.equal(resultShapeError(null), "result is not an object");
  assert.equal(resultShapeError([]), "result is not an object");
  assert.equal(resultShapeError({ valid: "yes", blocks: [] }), "field valid is not a boolean");
  assert.equal(resultShapeError({ valid: true }), "field blocks is not an array");
  assert.equal(resultShapeError({ valid: true, blocks: [1] }), "a block is not an object");
  assert.equal(resultShapeError({ valid: true, blocks: [{ id: 1, format: "html", start: 0, end: 0 }] }), "a block has no string id or format");
  assert.equal(resultShapeError({ valid: true, blocks: [{ id: "a", format: "html", start: 0.5, end: 1 }] }), "a block has no integer start or end");
  assert.equal(resultShapeError({ valid: false, blocks: [blk("a", 0, 0)] }), "an invalid result has blocks");
  assert.equal(compareWithView({ valid: 1 }, view([])).status, "bad-result");
});

test("candidates that agree give no difference; error codes are not compared", () => {
  const m = new Map([
    ["a", { valid: false, error: "RQP_MARKUP_STRUCTURE", blocks: [] }],
    ["b", { valid: false, error: "RQP_MARKUP_NESTED", blocks: [] }],
  ]);
  assert.deepEqual(compareCandidates(m), []);
});

test("candidates that differ in validity or blocks are grouped", () => {
  const m = new Map([
    ["a", valid([blk("x", 1, 2)])],
    ["b", valid([blk("x", 1, 3)])],
    ["c", { valid: false, error: "RQP_MARKUP_STRUCTURE", blocks: [] }],
  ]);
  assert.deepEqual(compareCandidates(m), ["a: valid x/html/1/2", "b: valid x/html/1/3", "c: invalid"]);
  assert.deepEqual(compareCandidates(new Map([["a", valid([])]])), []);
});

test("show never returns more than 80 characters", () => {
  assert.equal(show("a".repeat(200)).length, 80);
  assert.equal(show("ab"), '"ab"');
  // Exactly 80 characters with the quotes: shown in full.
  assert.equal(show("c".repeat(78)), `"${"c".repeat(78)}"`);
});
