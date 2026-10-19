// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import { docSeed, generateDocument, makeRng } from "../src/generator.mjs";
import { PROFILES as S5_PROFILES, generateS5Document } from "../src/generator-s5.mjs";

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

test("the classic profile is the default and does not change", () => {
  for (let i = 0; i < 200; i++) {
    const s = docSeed(4, i);
    assert.equal(generateDocument(s, { profile: "classic" }), generateDocument(s));
  }
});

test("the review profile reaches the shapes of the security review", () => {
  const docs = Array.from({ length: 3000 }, (_, i) => generateDocument(docSeed(1, i), { profile: "review", maxTokens: 20 }));
  const has = (re) => docs.some((d) => re.test(d));
  assert.ok(has(/<(a|nobr)>(<div>){8}/), "formatting element over 8 or more special elements");
  assert.ok(has(/<a><(span|em|q)>(<\w+>){7}/), "a crossing below the formatting element");
  assert.ok(has(/<table><td><\/tbody>/), "stray tbody end tag after an implied tbody");
  assert.ok(has(/<table><td><\/tr>/), "stray tr end tag after an implied tr");
  assert.ok(has(/<noscript><button><\/noscript>/), "unbalanced noscript content");
  assert.ok(has(/(<span><div><\/span>){100}/), "a long run of ignored end tags");
  assert.ok(has(/<script type="application\/rıvqen-manifest\+json">/), "dotless i in a script type");
  assert.ok(has(/(<table><td>){60}/), "deep nested tables");
});

test("the review profile does not change with review2: a saved finding of seed 21 still reproduces", () => {
  // Seed 21, index 6906 (edge cases §6.6): an a, a crossing by span, then 8 or more ul elements.
  assert.match(generateDocument(docSeed(21, 6906), { maxTokens: 20, profile: "review" }), /<a><span>(<ul>){8}/);
});

test("the review2 profile reaches the shapes of round 2 of the security review", () => {
  const docs = Array.from({ length: 3000 }, (_, i) => generateDocument(docSeed(1, i), { profile: "review2", maxTokens: 20 }));
  const has = (re) => docs.some((d) => re.test(d));
  assert.ok(has(/(<form><object><form><\/form><\/object><\/form>){9}/), "nested forms in object, repeated");
  assert.ok(has(/<template><col><title><\/template><p data-rq-block/), "template, col and raw text");
  assert.ok(has(/<table><marquee><table><\/table><\/marquee><\/table>/), "foster-parented marquee with a table");
  assert.ok(has(/<td><noscript><td><\/td><\/noscript>/), "noscript in a cell with a table part");
  assert.ok(has(/<caption><noscript><col><\/noscript>/), "noscript in a caption with col");
});

test("the review3 profile reaches the template for shapes of H-43 and keeps review2 seeds", () => {
  const docs = Array.from({ length: 3000 }, (_, i) => generateDocument(docSeed(1, i), { profile: "review3", maxTokens: 20 }));
  const has = (re) => docs.some((d) => re.test(d));
  assert.ok(has(/data-rq-block="r\d+"><\?start name="m">x<\?end><\/(div|p|span)><template for="m">/), "start and end markers in a block");
  assert.ok(has(/<\?start name="m"><(div|p|span) data-rq-block="r\d+">y<\/\1><\?end><template/), "markers around a block");
  assert.ok(has(/<template for>/) && has(/<template for="">/) && has(/<template FOR="m">/), "empty, valueless and upper-case for");
  assert.ok(has(/<svg><template for="m">/), "svg control");
  // review2 documents do not change when review3 exists (saved findings of seeds 51 and 52 stay valid).
  for (let i = 0; i < 200; i++) {
    const s = docSeed(51, i);
    assert.doesNotMatch(generateDocument(s, { profile: "review2", maxTokens: 20 }), /<template (for|FOR)/);
  }
});

test("S5 profiles: known answer (the documents of the S5 study generator), and unknown profiles throw", () => {
  // generator-s5.mjs is a copy of study/s5/src/gen-s5.mjs: the same seeds give the study documents.
  const digest = (profile) => {
    const h = createHash("sha256");
    for (let i = 0; i < 200; i++) h.update(`${generateS5Document(docSeed(7, i), profile)}\n`);
    return h.digest("hex").slice(0, 16);
  };
  assert.deepEqual(S5_PROFILES, ["tpl", "nos", "mix"]);
  assert.deepEqual(S5_PROFILES.map(digest), ["9a7e89eeca02990d", "150d9f4611c59876", "7748ef5fa59e06bc"]);
  assert.throws(() => generateS5Document(1, "classic"), RangeError);
});

test("S5 profiles reach template and noscript in block content with the shapes of H-94 to H-97", () => {
  const docs = S5_PROFILES.flatMap((p) => Array.from({ length: 2000 }, (_, i) => generateS5Document(docSeed(1, i), p)));
  const has = (re) => docs.some((d) => re.test(d));
  assert.ok(has(/data-rq-block="b\d+">[^]*<template[ >]/), "template in block content");
  assert.ok(has(/<template[^>]*shadowrootmode/i), "shadowrootmode");
  assert.ok(has(/<template[^>]*SHADOWROOTMODE/), "upper-case shadowrootmode");
  assert.ok(has(/<template>(<tr>|<td>|<caption>|<colgroup>|<tbody>)/), "a table part as the first child of a template");
  assert.ok(has(/<template><table>/), "a table in a template");
  assert.ok(has(/<template for="m">/), "template for");
  assert.ok(has(/data-rq-block="b\d+">[^]*<noscript>/), "noscript in block content");
  assert.ok(has(/<p data-rq-block="b\d+">[^<]*<noscript>[^]*<div>/), "a PCLOSE tag in C of a p block");
  assert.ok(has(/<noscript><template>/) && has(/<template><noscript>/), "nested template and noscript");
  assert.ok(has(/<noscript>[^]*<\/div>[^]*<\/noscript>/), "an end tag of a block element in C");
  for (const d of docs) assert.ok(d.length < 20_000);
});

test("review documents are bounded", () => {
  for (let i = 0; i < 1000; i++) {
    assert.ok(generateDocument(docSeed(9, i), { profile: "review", maxTokens: 200 }).length < 200 * 8000);
  }
});
