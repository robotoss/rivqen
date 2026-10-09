// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { docSeed, generateDocument, makeRng } from "../src/generator.mjs";

test("makeRng is deterministic and in [0, 1)", () => {
  const a = makeRng(42);
  const b = makeRng(42);
  for (let i = 0; i < 1000; i++) {
    const x = a();
    assert.equal(x, b());
    assert.ok(x >= 0 && x < 1);
  }
  assert.notEqual(makeRng(1)(), makeRng(2)());
});

test("known answer: the first values of seed 1 do not change between versions", () => {
  // A changed PRNG would make saved sweep findings (seed, index) impossible to reproduce.
  // Reference values of mulberry32 for seed 1: 0.6270739405881613, 0.002735721180215478.
  const r = makeRng(1);
  assert.deepEqual([r(), r()].map((x) => Math.floor(x * 2 ** 32)), [2693262067, 11749833]);
});

test("known answer: docSeed does not change between versions", () => {
  // Saved findings name (seed, index); the document seed must stay the same.
  assert.deepEqual([docSeed(0, 0), docSeed(1, 0), docSeed(5, 11420)], [docSeedRef(0, 0), docSeedRef(1, 0), docSeedRef(5, 11420)]);
});

/** Reference computation with BigInt (independent of Math.imul). */
function docSeedRef(seed, index) {
  const m = (a, b) => Number((BigInt(a) * BigInt(b)) % 2n ** 32n);
  return ((m(seed, 0x9e3779b1) ^ m(index + 1, 0x85ebca6b)) >>> 0);
}

test("docSeed gives different seeds for different indexes and sweeps", () => {
  const seen = new Set();
  for (let s = 0; s < 4; s++) for (let i = 0; i < 1000; i++) seen.add(docSeed(s, i));
  assert.equal(seen.size, 4000);
});

test("the same seed gives the same document", () => {
  assert.equal(generateDocument(7), generateDocument(7));
  assert.equal(generateDocument(7, { maxTokens: 40 }), generateDocument(7, { maxTokens: 40 }));
});

test("documents are bounded by maxTokens", () => {
  for (let i = 0; i < 2000; i++) {
    assert.ok(generateDocument(docSeed(9, i), { maxTokens: 1 }).length < 400);
    assert.ok(generateDocument(docSeed(9, i), { maxTokens: 200 }).length < 200 * 160);
  }
  // Out-of-range values are clamped.
  assert.equal(generateDocument(3, { maxTokens: 0 }), generateDocument(3, { maxTokens: 1 }));
  assert.equal(generateDocument(3, { maxTokens: 10_000 }), generateDocument(3, { maxTokens: 200 }));
});

test("the generator reaches the shapes that the sweep needs", () => {
  const docs = Array.from({ length: 3000 }, (_, i) => generateDocument(docSeed(1, i)));
  const has = (re) => docs.some((d) => re.test(d));
  assert.ok(has(/data-rq-block="b\d+"/), "html blocks");
  assert.ok(has(/<script type="application\/json" data-rq-block/), "json blocks");
  assert.ok(has(/<li><div>/), "list motif");
  assert.ok(has(/<table><tr><td>/), "table motif");
  assert.ok(has(/<noscript><script type="application\/rivqen-manifest\+json">/), "manifest in noscript");
  assert.ok(has(/<!\[CDATA\[x$/), "CDATA at EOF");
  assert.ok(has(/<svg><desc>/), "foreign integration point");
});
