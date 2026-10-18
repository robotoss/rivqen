// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// With the flag off (noscript 'shipped', head rule 'shipped') the study code gives
// the shipped results: the Node.js candidate on every golden markup fixture, and the
// head walk of the protocol reference and of the server demo on every golden and
// protocol fixture document.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze as analyzeCandidate } from '../../../node/src/markup.mjs';
import { documentHeadEnd as referenceHeadEnd } from '../../../../rqp-protocol/src/markup.mjs';
import { PAGE_IDS, pageBytes } from '../../../../rqp-protocol/src/pages.mjs';
import { documentHeadEnd as demoHeadEnd } from '../../../../../examples/server-demo/src/markup.js';
import { generateDocument } from '../../../crosscheck/src/generator.mjs';
import { analyzeStudy } from '../src/markup-study.mjs';
import { documentHeadEndStudy } from '../src/head-study.mjs';
import { docSeed, generateNoscriptDocument } from '../src/generator.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..', '..', '..', '..');
const MARKUP_FIXTURES = path.join(REPO, 'fixtures', 'rqp', 'markup');
const PROTOCOL_FIXTURES = path.join(REPO, 'fixtures', 'rqp', 'protocol');
const EXAMPLES = path.join(REPO, 'spec', 'rqp', 'examples');

function markupInputs() {
  return readdirSync(MARKUP_FIXTURES)
    .sort()
    .map((n) => [n, path.join(MARKUP_FIXTURES, n, 'input.html')])
    .filter(([, f]) => existsSync(f))
    .map(([n, f]) => [n, readFileSync(f)]);
}

/** Every document of the protocol fixtures: pages (files and recipes), response bodies, spec examples. */
function protocolInputs() {
  const out = PAGE_IDS.map((id) => [`page:${id}`, pageBytes(id)]);
  for (const n of readdirSync(PROTOCOL_FIXTURES).sort()) {
    const f = path.join(PROTOCOL_FIXTURES, n, 'response.body');
    if (existsSync(f)) out.push([`body:${n}`, readFileSync(f)]);
  }
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.html')) out.push([`spec:${path.relative(EXAMPLES, p)}`, readFileSync(p)]);
    }
  };
  walk(EXAMPLES);
  return out;
}

function assertHeadEqual(name, bytes) {
  const want = referenceHeadEnd(bytes);
  assert.equal(demoHeadEnd(bytes), want, `${name}: demo and reference differ (not a study defect)`);
  assert.equal(documentHeadEndStudy(bytes, 'shipped'), want, `${name}: study head walk`);
}

test('the shipped model equals the Node.js candidate on every golden markup fixture', () => {
  const inputs = markupInputs();
  assert.ok(inputs.length >= 190, `only ${inputs.length} fixtures`);
  for (const [name, bytes] of inputs) {
    assert.deepEqual(analyzeStudy(bytes, { noscript: 'shipped' }).result, analyzeCandidate(bytes), name);
  }
});

test('the shipped head rule equals the reference and demo head walk on golden and protocol fixtures', () => {
  const inputs = [...markupInputs(), ...protocolInputs()];
  assert.ok(inputs.length >= 280, `only ${inputs.length} documents`);
  let found = 0;
  for (const [name, bytes] of inputs) {
    assertHeadEqual(name, bytes);
    if (referenceHeadEnd(bytes) >= 0) found++;
  }
  assert.ok(found > 100, 'the inputs must reach the head end tag');
});

test('the shipped flags equal the shipped code on generated noscript and review2 documents', () => {
  for (let i = 0; i < 1500; i++) {
    for (const profile of ['body', 'head']) {
      const bytes = Buffer.from(generateNoscriptDocument(7, i, profile).input, 'utf8');
      assert.deepEqual(analyzeStudy(bytes).result, analyzeCandidate(bytes), `${profile} ${i}`);
      assertHeadEqual(`${profile} ${i}`, bytes);
    }
    const r2 = Buffer.from(generateDocument(docSeed(7, i), { maxTokens: 20, profile: 'review2' }), 'utf8');
    assert.deepEqual(analyzeStudy(r2).result, analyzeCandidate(r2), `review2 ${i}`);
  }
});
