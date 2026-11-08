// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Function fixtures (src/functions.mjs) on synthetic input: the errors of the case file and of
// an evidence file (the text of a check 12 problem), malformed function inputs, and the
// platform entries for evidence lines of each kind.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { functionFixture, parseCases, parseEvidence } from '../src/functions.mjs';
import { ROOT } from './client-helpers.mjs';

const CASES = parseCases(readFileSync(join(ROOT, 'tools/upstream-lab/client/functions.json'), 'utf8')).cases;
const byId = (id) => CASES.find((c) => c.id === id);
const canon = (fn, input) => functionFixture('legacy-canonical', [{ id: 'T', fn, input }], {}).file.results[0];
const platform = (profile, c, line, key = profile === 'legacy-android' ? 'android' : 'ios') => functionFixture(profile, [c], { [key]: new Map(line === undefined ? [] : [[c.id, line]]) });

test('a_case_file_error_names_the_defect_and_the_case', () => {
  const ok = (id) => ({ id, fn: 'etag', input: { value: 'x' } });
  assert.deepEqual(parseCases('{'), { error: 'the case file is not JSON' });
  assert.deepEqual(parseCases('{}'), { error: 'the case file must be an array of at most 4096 cases' });
  const max = Array.from({ length: 4096 }, (_, i) => ok(`C-${i}`));
  assert.equal(parseCases(JSON.stringify(max)).cases.length, 4096);
  assert.deepEqual(parseCases(JSON.stringify([...max, ok('C-x')])), { error: 'the case file must be an array of at most 4096 cases' });
  for (const bad of [null, [], { fn: 'etag', input: {} }, { id: 1, fn: 'etag', input: {} }, { id: 'b', fn: 'nope', input: {} }, { id: 'b', fn: 'etag', input: null }, { id: 'b', fn: 'etag', input: [] }]) {
    assert.deepEqual(parseCases(JSON.stringify([ok('a'), bad])), { error: 'case 2: needs id, a known fn and an input object' }, JSON.stringify(bad));
  }
  assert.deepEqual(parseCases(JSON.stringify([ok('a'), ok('a')])), { error: 'case a: the ID repeats' });
});

test('an_evidence_file_error_names_the_line', () => {
  assert.deepEqual(parseEvidence('{"id":"a"}\n   \n\n{"id":"b"}\n').lines.size, 2);
  assert.deepEqual(parseEvidence('{"id":"a"}\nnot json\n'), { error: 'line 2 is not JSON' });
  for (const bad of ['null', '[]', '{"id":1}', '"a"']) assert.deepEqual(parseEvidence(`{"id":"a"}\n${bad}\n`), { error: 'line 2 has no id' }, bad);
  assert.deepEqual(parseEvidence('{"id":"a"}\n', 'name'), { error: 'line 1 has no name' });
  assert.deepEqual([...parseEvidence('{"name":"n","v":1}\n', 'name').lines.keys()], ['n']);
});

test('canonical_functions_take_malformed_inputs_without_an_exception', () => {
  assert.equal(canon('split', { html: 1 }).error, 'not-a-string');
  assert.deepEqual(canon('diff', { local: {}, server: 'x' }).result, { diff: {} });
  // Field lists: pairs of strings only; an object maps names to string values.
  const json = (fields) => canon('message', { src_code: 200, final_code: 304, diff: {}, fields }).result.json;
  const clean = json([['etag', 'e']]);
  assert.equal(json([['etag', 'e'], ['template-tag'], ['template-tag', 't', 'x'], [1, 't'], ['template-tag', 1], 'template-tag']), clean);
  assert.equal(json({ etag: 'e', 'template-tag': 1 }), clean);
  assert.equal(json('etag'), json([]));
});

test('a_platform_entry_follows_the_kind_of_its_evidence_line', () => {
  const etag = { id: 'X-1', fn: 'etag', input: { value: 'x' } };
  // No evidence file at all: each case has no evidence line.
  assert.deepEqual(functionFixture('legacy-android', [etag], {}).problems, ['functions/legacy-android.json: X-1: no evidence line']);
  assert.deepEqual(functionFixture('legacy-ios', [{ ...etag, fn: 'template_change' }], {}).problems, ['functions/legacy-ios.json: X-1: no evidence line']);
  // A line without result, error or unsupported.
  assert.deepEqual(platform('legacy-ios', etag, { id: 'X-1' }).problems, ['functions/legacy-ios.json: X-1: the evidence line has no result, error or unsupported']);
  // Unsupported without a FACT row: none.
  for (const profile of ['legacy-android', 'legacy-ios']) {
    assert.deepEqual(platform(profile, etag, { unsupported: 'x' }).file.results, [{ id: 'X-1', fn: 'etag', evidence: 'none', result: null, note: 'no evidence and no FACT row' }], profile);
  }
  // iOS template_change: a bool_value that is not a boolean is unsupported.
  const tchange = { id: 'X-2', fn: 'template_change', input: { value: '1' } };
  assert.deepEqual(platform('legacy-ios', tchange, { id: 'X-2', bool_value: 'yes' }, 'boolvalue').file.results, [{ id: 'X-2', fn: 'template_change', evidence: 'none', result: null, note: 'no evidence and no FACT row' }]);
  // A trace result keeps only the members of the README table.
  assert.deepEqual(platform('legacy-ios', { id: 'X-3', fn: 'split', input: { html: '' } }, { result: { template: 't', extra: 1 } }).file.results[0].result, { template: 't' });
  assert.deepEqual(platform('legacy-ios', { id: 'X-4', fn: 'message', input: {} }, { result: { json: '{}', code: 1 } }).file.results[0].result, { json: '{}' });
});

test('the_android_model_is_checked_against_each_function_that_the_session_model_uses', () => {
  for (const id of ['FN-ID-01', 'FN-SPLIT-01', 'FN-DIFF-01']) {
    const c = byId(id);
    assert.ok(c !== undefined, id);
    const differs = platform('legacy-android', c, { id, result: { id: 'other', template: 'other', data: {}, diff: { other: 'x' } } }).problems;
    assert.equal(differs.length, 1, id);
    assert.match(differs[0], new RegExp(`^functions/legacy-android.json: ${id}: the Android model gives `));
    // An error line of a function that the model computes is a difference too, and the problem shows the error.
    const error = platform('legacy-android', c, { id, error: 'java.lang.Exception: x' }).problems;
    assert.match(error[0], /the evidence "java\.lang\.Exception"$/, id);
  }
  // A value of another JSON type is a difference, not an exception.
  assert.equal(platform('legacy-android', byId('FN-DIFF-01'), { id: 'FN-DIFF-01', result: { diff: null } }).problems.length, 1);
});
