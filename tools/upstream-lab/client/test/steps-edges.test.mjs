// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { normalizeEvent } from '../src/determinism.mjs';
import { scenarioProblems, stepProblem } from '../src/steps.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const ok = JSON.parse(readFileSync(path.join(here, '..', 'scenarios.json'), 'utf8'))[0];
const problems = (patch) => scenarioProblems({ ...ok, ...patch });

test('scenario_id_must_match_the_whole_pattern', () => {
  for (const id of ['XCS-01', 'CS-01x9', 'CS-1', 'CS-001', 'cs-01', 'CS-01A', ' CS-01', 'CS-01\n']) assert.ok(problems({ id }).length > 0, JSON.stringify(id));
  for (const id of ['CS-01', 'CS-13a']) assert.deepEqual(problems({ id }), [], id);
});

test('case_page_must_match_the_whole_page_pattern', () => {
  for (const page of ['a.b', 'a/b', '../a', '', 'A', 'a b', 'a\n']) assert.ok(problems({ case: { page } }).length > 0, JSON.stringify(page));
  for (const page of ['a', 'titleattr', 'a-b_1']) assert.deepEqual(problems({ case: { page } }), [], page);
});

test('case_headers_must_all_be_pairs_of_strings', () => {
  assert.deepEqual(problems({ case: { page: 'a', headers: [['a', 'b'], ['c', 'd']] } }), []);
  assert.ok(problems({ case: { page: 'a', headers: [['a', 'b'], ['c']] } }).length > 0);
  assert.ok(problems({ case: { page: 'a', headers: [['a', 'b'], ['c', 1]] } }).length > 0);
  assert.ok(problems({ case: { page: 'a', headers: [['a', 1], ['c', 'd']] } }).length > 0);
  assert.ok(problems({ case: { page: 'a', headers: 'x' } }).length > 0);
  assert.ok(problems({ case: [] }).length > 0);
});

test('scenario_problems_name_the_scenario_and_the_member', () => {
  assert.deepEqual(problems({ title: '' }), ['CS-01: title is required']);
  assert.deepEqual(scenarioProblems({ ...ok, id: undefined }).slice(0, 1), ['?: id must be CS-<NN> with an optional letter']);
  assert.deepEqual(problems({ modes: ['quick', 'fast'] }), ['CS-01: modes must be a non-empty subset of quick, standard']);
  assert.deepEqual(problems({ rows: ['Q1', 2] }), ['CS-01: rows must be a list of strings']);
  assert.deepEqual(problems({ config: null }), ['CS-01: config must be an object']);
  assert.deepEqual(problems({ config: 'x' }), ['CS-01: config must be an object']);
  assert.deepEqual(problems({ title: 5 }), ['CS-01: title is required']);
  assert.deepEqual(problems({ setup: [null] }), ['CS-01: setup[0]: step must be an object']);
  assert.deepEqual(problems({ setup: [{ case: { page: 'a' }, extra: 1 }] }), ['CS-01: setup[0]: unknown step "undefined"']);
});

test('step_set_cookie_needs_a_string_value', () => {
  assert.equal(stepProblem({ do: 'set_cookie', value: 'a=b' }), null);
  assert.equal(stepProblem({ do: 'set_cookie', value: 3 }), 'set_cookie: value must be a string');
  assert.equal(stepProblem({ do: 'set_cookie' }), 'set_cookie: value must be a string');
  assert.equal(stepProblem(5), 'step must be an object');
});

test('determinism_only_blanks_pairs_of_two_strings', () => {
  const e = normalizeEvent({ ev: 'x', src: 'proxy', a: ['date', 'v', 'w'], b: ['date'], c: [['date', 'v']], d: ['date', 1] });
  assert.deepEqual([e.a, e.b, e.c, e.d], [['date', 'v', 'w'], ['date'], [['date', '']], ['date', 1]]);
});

test('determinism_leaves_other_messages_and_snapshots_alone', () => {
  assert.deepEqual(normalizeEvent({ ev: 'session.result', json: '{"local_refresh_time":1}' }).json, '{"local_refresh_time":1}');
  assert.deepEqual(normalizeEvent({ ev: 'session.created', files: [{ name: 'a.header', text: 'Date: x' }] }).files, [{ name: 'a.header', text: 'Date: x' }]);
  assert.deepEqual(normalizeEvent({ ev: 'cache.snapshot', files: 'none' }).files, 'none');
  assert.deepEqual(normalizeEvent({ ev: 'cache.snapshot', files: [null, { size: 1 }] }).files, [null, { size: 1 }]);
});
