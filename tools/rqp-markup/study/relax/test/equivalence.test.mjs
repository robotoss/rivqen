// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The study rule with every flag off is the markup rule set of S2 (before the
// S3 decisions H-71, H-73, H-74, H-75). It gives the result object (CONTRACT.md
// section 3) of the pre-S3 Node.js candidate, recorded in
// ../../baseline/baseline-s2.json: on every S2 golden fixture, on every input of
// the S2 cross-check cases and on generated documents of all generator profiles.
// (Until S3 these tests compared with the shipped candidate and with
// expected.json; both follow the S3 rules now.)

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BASE, COMBOS, analyzeRelaxed, flagsOf } from '../src/markup-relax.mjs';
import { contractFields } from '../src/combos.mjs';
import { digestResults, loadBaseline } from '../../baseline/baseline.mjs';
import { GROUPS } from '../../baseline/groups.mjs';

const BASELINE = loadBaseline();
const flagsOff = (bytes) => contractFields(analyzeRelaxed(bytes, BASE));

test('flags off: equal to the pre-S3 candidate on every S2 golden fixture', () => {
  const names = Object.keys(BASELINE.fixtures);
  assert.equal(names.length, 197, 'the 197 golden fixtures of S2');
  const list = GROUPS.fixtures(names);
  assert.equal(list.length, 197);
  for (const [name, bytes] of list) assert.deepEqual(flagsOff(bytes), BASELINE.fixtures[name], name);
});

for (const group of ['cases', 'classic', 'review', 'review2', 'review3', 'relax']) {
  test(`flags off: equal to the pre-S3 candidate on the ${group} group`, () => {
    const list = GROUPS[group]();
    assert.equal(list.length, BASELINE.groups[group].count);
    assert.equal(digestResults(list.map(([, b]) => flagsOff(b))), BASELINE.groups[group].digest);
  });
}

test('flagsOf: every combination name gives its flags; unknown names throw', () => {
  assert.deepEqual(flagsOf('base'), BASE);
  assert.deepEqual(flagsOf('r1+r3'), { r1: true, r1Allow: false, r2: false, r3: true, r3Scope: 'root' });
  assert.deepEqual(flagsOf('r2+r3w'), { r1: false, r1Allow: false, r2: true, r3: true, r3Scope: 'region' });
  assert.deepEqual(flagsOf('r1a'), { r1: true, r1Allow: true, r2: false, r3: false, r3Scope: 'root' });
  for (const c of COMBOS) assert.doesNotThrow(() => flagsOf(c));
  assert.throws(() => flagsOf('r4'), RangeError);
});
