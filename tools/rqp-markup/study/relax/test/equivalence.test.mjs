// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The study rule with every flag off is the shipped Node.js candidate: the same
// result object (CONTRACT.md section 3) on every golden fixture, on every input
// of the cross-check cases and on generated documents of all generator profiles.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { analyze } from '../../../node/src/markup.mjs';
import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { BASE, COMBOS, analyzeRelaxed, flagsOf } from '../src/markup-relax.mjs';
import { generateRelaxDocument } from '../src/gen.mjs';
import { contractFields } from '../src/combos.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..', '..', '..', '..');
const FIXTURES = path.join(ROOT, 'fixtures', 'rqp', 'markup');
const CASES = path.join(ROOT, 'tools', 'rqp-markup', 'crosscheck', 'cases');

function fixtures() {
  return readdirSync(FIXTURES)
    .filter((name) => existsSync(path.join(FIXTURES, name, 'input.html')))
    .sort()
    .map((name) => ({ name, bytes: readFileSync(path.join(FIXTURES, name, 'input.html')) }));
}

function cases() {
  const out = [];
  for (const file of readdirSync(CASES).filter((f) => f.endsWith('.json')).sort()) {
    for (const c of JSON.parse(readFileSync(path.join(CASES, file), 'utf8'))) {
      out.push({ name: `${file}:${c.id}`, bytes: Buffer.from(c.input, 'utf8') });
    }
  }
  return out;
}

test('flags off: equal to the shipped candidate on every golden fixture', () => {
  const list = fixtures();
  assert.ok(list.length >= 197, `expected the 197 golden fixtures (788 runner checks / 4), got ${list.length}`);
  for (const { name, bytes } of list) {
    assert.deepEqual(contractFields(analyzeRelaxed(bytes, BASE)), analyze(bytes), name);
  }
});

test('flags off: equal to the expected.json of every golden fixture', () => {
  for (const { name } of fixtures()) {
    const bytes = readFileSync(path.join(FIXTURES, name, 'input.html'));
    const expected = JSON.parse(readFileSync(path.join(FIXTURES, name, 'expected.json'), 'utf8'));
    const got = contractFields(analyzeRelaxed(bytes, BASE));
    assert.equal(got.valid, expected.valid, name);
    if (expected.valid) {
      assert.deepEqual(got.blocks, expected.blocks, name);
      assert.equal(got.template_revision, expected.template_revision, name);
      assert.equal(got.page_revision, expected.page_revision, name);
    } else {
      assert.equal(got.error, expected.error, name);
    }
  }
});

test('flags off: equal to the shipped candidate on the cross-check cases', () => {
  const list = cases();
  assert.ok(list.length > 100);
  for (const { name, bytes } of list) {
    assert.deepEqual(contractFields(analyzeRelaxed(bytes, BASE)), analyze(bytes), name);
  }
});

test('flags off: equal to the shipped candidate on generated documents of every profile', () => {
  const n = Number(process.env.RQP_RELAX_EQ_DOCS ?? 1500);
  for (const profile of ['classic', 'review', 'review2', 'review3']) {
    for (let i = 0; i < n; i++) {
      const bytes = Buffer.from(generateDocument(docSeed(7, i), { maxTokens: 20, profile }), 'utf8');
      assert.deepEqual(contractFields(analyzeRelaxed(bytes, BASE)), analyze(bytes), `${profile} ${i}`);
    }
  }
  for (let i = 0; i < n; i++) {
    const bytes = Buffer.from(generateRelaxDocument(docSeed(8, i)), 'utf8');
    assert.deepEqual(contractFields(analyzeRelaxed(bytes, BASE)), analyze(bytes), `relax ${i}`);
  }
});

test('flagsOf: every combination name gives its flags; unknown names throw', () => {
  assert.deepEqual(flagsOf('base'), BASE);
  assert.deepEqual(flagsOf('r1+r3'), { r1: true, r1Allow: false, r2: false, r3: true, r3Scope: 'root' });
  assert.deepEqual(flagsOf('r2+r3w'), { r1: false, r1Allow: false, r2: true, r3: true, r3Scope: 'region' });
  assert.deepEqual(flagsOf('r1a'), { r1: true, r1Allow: true, r2: false, r3: false, r3Scope: 'root' });
  for (const c of COMBOS) assert.doesNotThrow(() => flagsOf(c));
  assert.throws(() => flagsOf('r4'), RangeError);
});
