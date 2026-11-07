// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Function fixtures (README "Function fixtures"): canonical results from CL-14, CL-15, CL-16,
// CL-24, CL-41, CL-42, CL-43; platform results from the evidence; the Android model checked
// against the evidence.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { functionFixture, parseCases, parseEvidence } from '../src/functions.mjs';
import { ROOT } from './client-helpers.mjs';

const read = (p) => readFileSync(join(ROOT, p), 'utf8');
const CASES = parseCases(read('tools/upstream-lab/client/functions.json')).cases;
const EVIDENCE = {
  android: parseEvidence(read('evidence/traces/client/android/functions.jsonl')).lines,
  ios: parseEvidence(read('evidence/traces/client/ios/functions.jsonl')).lines,
  boolvalue: parseEvidence(read('evidence/traces/client/ios/boolvalue.jsonl')).lines,
};
const one = (profile, id) => functionFixture(profile, CASES.filter((c) => c.id === id), EVIDENCE).file.results[0];
const canon = (fn, input) => functionFixture('legacy-canonical', [{ id: 'T', fn, input }], {}).file.results[0];

test('every_profile_file_has_one_entry_per_case_and_no_problem', () => {
  for (const profile of ['legacy-canonical', 'legacy-android', 'legacy-ios']) {
    const { file, problems } = functionFixture(profile, CASES, EVIDENCE);
    assert.deepEqual(problems, []);
    assert.deepEqual(file.results.map((r) => r.id), CASES.map((c) => c.id));
  }
});

test('android_model_differs_from_the_evidence_is_a_problem', () => {
  const ev = { ...EVIDENCE, android: new Map(EVIDENCE.android) };
  ev.android.set('FN-ETAG-15', { ...ev.android.get('FN-ETAG-15'), result: { value: 'other' } });
  const { problems } = functionFixture('legacy-android', CASES.filter((c) => c.id === 'FN-ETAG-15'), ev);
  assert.match(problems[0], /FN-ETAG-15: the Android model gives/);
});

test('a_case_without_an_evidence_line_is_a_problem', () => {
  const { problems } = functionFixture('legacy-ios', [{ id: 'FN-NEW', fn: 'etag', input: { value: 'x' } }], EVIDENCE);
  assert.match(problems[0], /FN-NEW: no evidence line/);
});

test('cl41_canonical_split_and_its_failure_token', () => {
  assert.deepEqual(canon('split', { html: '<title>T</title><!--sonicdiff-a-->A<!--sonicdiff-a-end-->' }).result, { template: '{title}{a}', data: { '{title}': '<title>T</title>', '{a}': '<!--sonicdiff-a-->A<!--sonicdiff-a-end-->' } });
  assert.equal(canon('split', { html: '<!--sonicdiff-a-->A<!--sonicdiff-a-end--><!--sonicdiff-a-->B<!--sonicdiff-a-end-->' }).error, 'duplicate-block-name');
});

test('cl42_canonical_diff_new_keys_and_value_not_string', () => {
  assert.deepEqual(canon('diff', { local: { '{a}': 'x' }, server: { '{a}': 'x', '{b}': '' } }).result, { diff: { '{b}': '' } });
  assert.deepEqual(canon('diff', { local: null, server: { '{a}': 'x' } }).result, { diff: { '{a}': 'x' } });
  assert.equal(canon('diff', { local: {}, server: { '{a}': 1 } }).error, 'value-not-string');
});

test('cl24_cl28_canonical_message_none_for_minus_one_or_an_empty_diff', () => {
  assert.equal(canon('message', { src_code: -1, final_code: -1, diff: {}, fields: [] }).result.json, null);
  assert.equal(canon('message', { src_code: 200, final_code: 200, diff: {}, fields: [] }).result.json, null);
  assert.equal(canon('message', { src_code: 200, final_code: 200, diff: { '{a}': 'a/b' }, fields: { etag: 'e1' } }).result.json, '{"result":"{\\"{a}\\":\\"a/b\\"}","code":200,"srcCode":200,"extra":{"eTag":"e1","isReload":"false"}}');
});

test('cl14_cl15_cl16_canonical_field_functions', () => {
  assert.equal(canon('is_legacy_response', { fields: [['etag', 'e'], ['template-tag', 't'], ['template-change', 'true'], ['cache-offline', 'true']] }).result.value, true);
  assert.equal(canon('is_legacy_response', { fields: [['etag', 'e'], ['template-tag', 't'], ['template-change', 'true'], ['cache-offline', '']] }).result.value, false);
  assert.equal(canon('template_change', { value: '0' }).result.value, false);
  assert.equal(canon('template_change', { value: 'FALSE' }).result.value, true);
  assert.equal(canon('etag', { value: 'W/"abc"' }).result.value, 'abc');
  assert.equal(canon('etag', { value: 'w/"abc"' }).result.value, 'w/"abc"');
  assert.equal(canon('charset', { content_type: 'text/html' }).result.value, 'utf-8');
  assert.equal(canon('charset', { content_type: 'text/html; charset="GBK"' }).result.value, 'gbk');
});

test('cl43_canonical_session_id_has_no_value', () => {
  assert.deepEqual(one('legacy-canonical', 'FN-ID-01'), { id: 'FN-ID-01', fn: 'session_id', evidence: 'none', result: null, note: 'CL-43: the canonical client computes no session id' });
});

test('android_unsupported_cases_are_code_reading_or_none', () => {
  const msg = one('legacy-android', 'FN-MSG-10');
  assert.equal(msg.evidence, 'code-reading');
  assert.match(msg.result.json, /café \\\\\\\/ /);
  assert.deepEqual(one('legacy-android', 'FN-TCHANGE-03').result, { value: false });
  assert.equal(one('legacy-android', 'FN-LEGACY-01').evidence, 'none');
});

test('ios_template_change_comes_from_boolvalue_and_errors_keep_the_class', () => {
  assert.deepEqual(one('legacy-ios', 'FN-TCHANGE-01'), { id: 'FN-TCHANGE-01', fn: 'template_change', evidence: 'trace', result: { value: true } });
  assert.equal(one('legacy-ios', 'FN-DIFF-07').error, 'NSInvalidArgumentException');
  assert.deepEqual(one('legacy-ios', 'FN-CHARSET-05').result, { value: 'GBK' });
  assert.equal(one('legacy-ios', 'FN-MSG-01').evidence, 'none');
});

test('a_bad_case_file_or_evidence_line_is_reported', () => {
  assert.ok(parseCases('{').error);
  assert.ok(parseCases('[{"id":"a","fn":"nope","input":{}}]').error);
  assert.ok(parseCases('[{"id":"a","fn":"etag","input":{}},{"id":"a","fn":"etag","input":{}}]').error);
  assert.ok(parseEvidence('{"id":"a"}\nnot json').error);
});
