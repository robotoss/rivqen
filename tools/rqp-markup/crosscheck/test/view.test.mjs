// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { parse5View } from "../src/view.mjs";

const b = (s) => Buffer.from(s, "utf8");
const ranges = (v) => v.blocks.map((x) => [x.id, x.start, x.end]);

test("block content offsets are UTF-8 byte offsets", () => {
  const doc = '<!DOCTYPE html><p>\u{1F600}é</p><div data-rq-block="a">€x</div>';
  const start = Buffer.byteLength('<!DOCTYPE html><p>\u{1F600}é</p><div data-rq-block="a">');
  const v = parse5View(b(doc));
  assert.equal(v.ok, true);
  assert.deepEqual(ranges(v), [["a", start, start + 4]]);
});

test("a BOM and CR LF count in the offsets", () => {
  const doc = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), b('<p>\r\n</p><span data-rq-block="s">\r\nx</span>')]);
  const start = 3 + Buffer.byteLength('<p>\r\n</p><span data-rq-block="s">');
  assert.deepEqual(ranges(parse5View(doc)), [["s", start, start + 3]]);
});

test("title and json script blocks have end tag offsets", () => {
  const doc = '<title data-rq-block="t">A</title><script type="application/json" data-rq-block="j">{}</script>';
  const v = parse5View(b(doc));
  assert.deepEqual(
    v.blocks.map((x) => [x.id, x.tag, x.type]),
    [["t", "title", null], ["j", "script", "application/json"]],
  );
  assert.equal(v.blocks[1].end - v.blocks[1].start, 2);
});

test("an implied end tag gives end null", () => {
  const v = parse5View(b('<p data-rq-block="a">one<p>two'));
  assert.equal(v.blocks[0].end, null);
});

test("blocks in template content and foreign content are reported", () => {
  const v = parse5View(b('<template><div data-rq-block="t">x</div></template><svg><title data-rq-block="s">y</title></svg>'));
  assert.deepEqual(
    v.blocks.map((x) => [x.id, x.html]),
    [["t", true], ["s", false]],
  );
});

test("the first of two data-rq-block attributes counts", () => {
  const v = parse5View(b('<div data-rq-block="a" DATA-RQ-BLOCK="b">x</div>'));
  assert.deepEqual(v.blocks.map((x) => x.id), ["a"]);
});

test("noscript content is markup only with scripting disabled", () => {
  const doc = b('<noscript><p data-rq-block="n">x</p><script type="application/rivqen-manifest+json">{}</script></noscript>');
  const on = parse5View(doc);
  const off = parse5View(doc, { scripting: false });
  assert.deepEqual([on.blocks.length, on.manifests], [0, 0]);
  assert.deepEqual([off.blocks.length, off.manifests], [1, 1]);
});

test("manifest-like scripts are counted (case-insensitive type, decoded references)", () => {
  const v = parse5View(b('<script type="Application/Rivqen-Manifest+JSON"></script><script type="application/rivqen-manifest&#43;json"></script><script type="application/json"></script>'));
  assert.equal(v.manifests, 2);
});

test("invalid UTF-8 and inputs over 5 MiB have no view", () => {
  assert.deepEqual(parse5View(Buffer.from([0x3c, 0xc3, 0x28])).ok, false);
  assert.deepEqual(parse5View(Buffer.alloc(5 * 1024 * 1024 + 1, 0x61)).ok, false);
  assert.deepEqual(parse5View(Buffer.alloc(5 * 1024 * 1024, 0x61)).ok, true);
});

test("deep nesting does not overflow the stack", () => {
  // The walk is iterative. parse5 itself needs time that grows faster than the depth
  // (about 25 s for 50 000 levels), so the test uses 8 000 levels.
  const depth = 8_000;
  const v = parse5View(b(`${"<div>".repeat(depth)}<span data-rq-block="d">x</span>`));
  assert.equal(v.blocks.length, 1);
});
