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
import { digestResults, loadBaseline } from '../../baseline/baseline.mjs';
import { GROUPS } from '../../baseline/groups.mjs';
import { documentHeadStart as referenceHeadStart } from '../../../../rqp-protocol/src/markup.mjs';
import { PAGE_IDS, pageBytes } from '../../../../rqp-protocol/src/pages.mjs';
import { documentHeadStart as demoHeadStart } from '../../../../../examples/server-demo/src/markup.js';
import { analyzeStudy } from '../src/markup-study.mjs';
import { headWalk } from '../src/head-study.mjs';

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

// H-72 shipped option hc: the reference and demo insertion point equals the study headStart.
function assertHeadEqual(name, bytes) {
  const want = referenceHeadStart(bytes);
  assert.equal(demoHeadStart(bytes), want, `${name}: demo and reference differ (not a study defect)`);
  assert.equal(headWalk(bytes).headStart, want, `${name}: study head walk (option hc)`);
}

// The shipped model is the S2 rule set. It equals the pre-S3 Node.js candidate, recorded in
// ../../baseline/baseline-s2.json (the shipped candidate follows the S3 rules now).
const BASELINE = loadBaseline();

test('the shipped model equals the pre-S3 Node.js candidate on every S2 golden markup fixture', () => {
  const list = GROUPS.fixtures(Object.keys(BASELINE.fixtures));
  assert.equal(list.length, 197);
  for (const [name, bytes] of list) {
    assert.deepEqual(analyzeStudy(bytes, { noscript: 'shipped' }).result, BASELINE.fixtures[name], name);
  }
});

test('the study option hc equals the shipped H-72 head start of the reference and the demo on golden and protocol fixtures', () => {
  const inputs = [...markupInputs(), ...protocolInputs()];
  assert.ok(inputs.length >= 280, `only ${inputs.length} documents`);
  let found = 0;
  for (const [name, bytes] of inputs) {
    assertHeadEqual(name, bytes);
    if (referenceHeadStart(bytes) >= 0) found++;
  }
  assert.ok(found > 100, 'the inputs must reach the head start tag');
});

test('the shipped flags equal the pre-S3 candidate on generated noscript and review2 documents', () => {
  for (const group of ['noscriptBody', 'noscriptHead', 'review2']) {
    const list = GROUPS[group]();
    assert.equal(list.length, BASELINE.groups[group].count);
    assert.equal(digestResults(list.map(([, b]) => analyzeStudy(b).result)), BASELINE.groups[group].digest, group);
  }
  for (const [name, bytes] of [...GROUPS.noscriptBody(), ...GROUPS.noscriptHead()]) assertHeadEqual(name, bytes);
});
