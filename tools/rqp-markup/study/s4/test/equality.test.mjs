// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// With the flags off the study code gives the pre-S4 results: the study copy of the
// Node.js candidate (close 'off') equals the pre-S4 Node.js candidate (commit 9bd8c32,
// recorded in ../../baseline/baseline-s3.json) on the 247 golden fixtures of that
// commit, the S3 cross-check cases and generated documents. The shipped candidate
// follows the S4 decisions (H-78, H-84, H-86, H-85a) now, so the test does not
// compare with it any more. The head rule 'shipped' (H-72) equals documentHeadStart of
// the frozen pre-S4 reference (../../baseline/reference-s3.mjs). The head rule 'h79'
// is the adopted H-79a and equals documentHeadStart of the shipped reference and of
// the server demo.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BASELINE_S3_FILE, digestResults, loadBaseline } from '../../baseline/baseline.mjs';
import { GROUPS_S3 } from '../../baseline/groups-s3.mjs';
import { documentHeadStart as referenceHeadStart } from '../../baseline/reference-s3.mjs';
import { documentHeadStart as shippedHeadStart } from '../../../../rqp-protocol/src/markup.mjs';
import { documentHeadStart as demoHeadStart } from '../../../../../examples/server-demo/src/markup.js';
import { analyzeS4, contractFields } from '../src/markup-s4.mjs';
import { headPosition } from '../src/head.mjs';
import { generateHeadDocument } from '../src/head-gen.mjs';
import { generateTableDocument } from '../src/table-gen.mjs';
import { crosscheckCases, markupFixtures, protocolDocuments } from '../src/inputs.mjs';

const off = (bytes) => contractFields(analyzeS4(bytes, { close: 'off' }));

test('close off equals the pre-S4 Node.js candidate on the 247 golden fixtures of the study', () => {
  const baseline = loadBaseline(BASELINE_S3_FILE);
  const names = Object.keys(baseline.fixtures);
  assert.equal(names.length, 247);
  const list = GROUPS_S3.fixtures(names);
  assert.equal(list.length, 247);
  for (const [name, bytes] of list) assert.deepEqual(off(bytes), baseline.fixtures[name], name);
});

test('close off equals the pre-S4 candidate on the S3 cross-check cases and on generated documents', () => {
  const baseline = loadBaseline(BASELINE_S3_FILE);
  let total = 0;
  for (const [group, make] of Object.entries(GROUPS_S3)) {
    if (group === 'fixtures') continue;
    const list = make();
    total += list.length;
    assert.equal(list.length, baseline.groups[group].count, group);
    assert.equal(digestResults(list.map(([, b]) => off(b))), baseline.groups[group].digest, group);
  }
  assert.ok(total >= 3800);
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

test('the head rule shipped equals the frozen pre-S4 reference; h79 equals the shipped reference and the demo', () => {
  const inputs = [...markupFixtures(), ...protocolDocuments(), ...crosscheckCases()];
  for (let i = 0; i < 3000; i++) inputs.push([`head ${i}`, Buffer.from(generateHeadDocument(6, i), 'utf8')]);
  let found = 0;
  for (const [name, bytes] of inputs) {
    // Changed: the reference and the demo follow H-79a now, so they are compared
    // with the option h79 (the adopted rule). The option shipped (H-72) is compared
    // with the frozen copy of the pre-S4 reference.
    const want = referenceHeadStart(bytes);
    assert.equal(headPosition(bytes, 'shipped').at, want, name);
    const now = shippedHeadStart(bytes);
    assert.equal(demoHeadStart(bytes), now, `${name}: demo and reference differ (not a study defect)`);
    assert.equal(headPosition(bytes, 'h79').at, now, `${name}: h79 differs from the shipped H-79a`);
    if (want >= 0) found++;
  }
  assert.ok(found > 300, 'the inputs must reach the head start tag');
});
