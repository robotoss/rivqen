// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { catalogProblems } from '../src/catalog.mjs';
import { expectProblem } from './helpers.mjs';

const row = (platform, verdict, results) => ({
  id: 'CAP-X-01',
  platform,
  title: 'Title',
  prototype: 'PROTO',
  questions: [],
  sprint: 'S1',
  results,
  verdict,
});
const result = (os, verdict, extra = {}) => ({ os, verdict, condition: null, runs: [], ...extra });

test('verdict_rules: a conditional result needs a condition', () => {
  const results = [result('host-jvm', 'conditional')];
  expectProblem(catalogProblems([row('ffi', 'conditional', results)]), /a conditional result needs a condition/);
});

test('verdict_rules: the capability verdict is the weakest result', () => {
  const results = [result('a', 'supported'), result('b', 'conditional', { condition: 'only on 2 GB' })];
  assert.deepEqual(catalogProblems([row('ffi', 'conditional', results)]), []);
  expectProblem(catalogProblems([row('ffi', 'supported', results)]), /verdict is supported, the results give conditional/);
});

test('verdict_rules: impossible is weaker than conditional', () => {
  const results = [result('a', 'impossible', { source: 'https://developer.android.com/x' }), result('b', 'supported')];
  assert.deepEqual(catalogProblems([row('ffi', 'impossible', results)]), []);
});

test('verdict_rules: an impossible result needs a source or a run', () => {
  const results = [result('host-jvm', 'impossible')];
  expectProblem(catalogProblems([row('ffi', 'impossible', results)]), /an impossible result needs a source or a run/);
});

test('verdict_rules: android stays pending until both H-128 versions have a result', () => {
  const only37 = [result('37', 'supported')];
  expectProblem(catalogProblems([row('android', 'supported', only37)]), /the results give pending/);
  assert.deepEqual(catalogProblems([row('android', 'pending', only37)]), []);
  const both = [result('37', 'supported'), result('29', 'supported')];
  assert.deepEqual(catalogProblems([row('android', 'supported', both)]), []);
});

test('verdict_rules: ios needs all three H-128 versions', () => {
  const two = [result('27.0', 'supported'), result('26.5', 'supported')];
  expectProblem(catalogProblems([row('ios', 'supported', two)]), /the results give pending/);
  const three = [...two, result('18.5', 'supported')];
  assert.deepEqual(catalogProblems([row('ios', 'supported', three)]), []);
});

test('verdict_rules: a not-tested result keeps the capability pending', () => {
  const results = [result('37', 'supported'), result('29', 'not-tested')];
  assert.deepEqual(catalogProblems([row('android', 'pending', results)]), []);
  expectProblem(catalogProblems([row('android', 'supported', results)]), /the results give pending/);
});

test('verdict_rules: ffi and url need at least one result for a non-pending verdict', () => {
  assert.deepEqual(catalogProblems([row('url', 'pending', [])]), []);
  expectProblem(catalogProblems([row('url', 'supported', [])]), /the results give pending/);
});

test('verdict_rules: a duplicate result os is reported', () => {
  const results = [result('37', 'supported'), result('37', 'supported')];
  expectProblem(catalogProblems([row('android', 'pending', results)]), /duplicate result os 37/);
});
