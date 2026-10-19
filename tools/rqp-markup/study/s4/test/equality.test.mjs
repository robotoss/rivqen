// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// With the flags off the study code gives the shipped results: the study copy of the
// Node.js candidate (close 'off') equals tools/rqp-markup/node on the 247 golden
// fixtures, every cross-check case and generated documents; the head rule 'shipped'
// equals documentHeadStart of the protocol reference and of the server demo.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyze } from '../../../node/src/markup.mjs';
import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { documentHeadStart as referenceHeadStart } from '../../../../rqp-protocol/src/markup.mjs';
import { documentHeadStart as demoHeadStart } from '../../../../../examples/server-demo/src/markup.js';
import { analyzeS4, contractFields } from '../src/markup-s4.mjs';
import { headPosition } from '../src/head.mjs';
import { generateHeadDocument } from '../src/head-gen.mjs';
import { generateTableDocument } from '../src/table-gen.mjs';
import { crosscheckCases, markupFixtures, protocolDocuments } from '../src/inputs.mjs';

const off = (bytes) => contractFields(analyzeS4(bytes, { close: 'off' }));

test('close off equals the shipped Node.js candidate on all 247 golden fixtures', () => {
  const list = markupFixtures();
  assert.equal(list.length, 247);
  for (const [name, bytes] of list) assert.deepEqual(off(bytes), analyze(bytes), name);
});

test('close off equals the shipped candidate on every cross-check case and on generated documents', () => {
  const docs = [...crosscheckCases()];
  for (const profile of ['classic', 'review', 'review2', 'review3']) {
    for (let i = 0; i < 400; i++) docs.push([`${profile} ${i}`, Buffer.from(generateDocument(docSeed(4, i), { maxTokens: 24, profile }), 'utf8')]);
  }
  for (let i = 0; i < 2000; i++) docs.push([`tables ${i}`, Buffer.from(generateTableDocument(4, i), 'utf8')]);
  assert.ok(docs.length >= 3800);
  for (const [name, bytes] of docs) assert.deepEqual(off(bytes), analyze(bytes), name);
});

test('the closure modes accept a superset of close off with the same result where off accepts', () => {
  for (let i = 0; i < 2000; i++) {
    const bytes = Buffer.from(generateTableDocument(5, i), 'utf8');
    const base = off(bytes);
    for (const close of ['b1', 'b2', 'b3']) {
      const r = contractFields(analyzeS4(bytes, { close }));
      if (base.valid) assert.deepEqual(r, base, `${close} document ${i}`);
    }
    const b1 = analyzeS4(bytes, { close: 'b1' });
    if (b1.valid) assert.deepEqual(contractFields(analyzeS4(bytes, { close: 'b2' })), contractFields(b1), `b1 within b2, document ${i}`);
  }
});

test('the head rule shipped equals the reference and the demo on fixtures, protocol documents and generated prefixes', () => {
  const inputs = [...markupFixtures(), ...protocolDocuments(), ...crosscheckCases()];
  for (let i = 0; i < 3000; i++) inputs.push([`head ${i}`, Buffer.from(generateHeadDocument(6, i), 'utf8')]);
  let found = 0;
  for (const [name, bytes] of inputs) {
    const want = referenceHeadStart(bytes);
    assert.equal(demoHeadStart(bytes), want, `${name}: demo and reference differ (not a study defect)`);
    assert.equal(headPosition(bytes, 'shipped').at, want, name);
    if (want >= 0) found++;
  }
  assert.ok(found > 300, 'the inputs must reach the head start tag');
});
