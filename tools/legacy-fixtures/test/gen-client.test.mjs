// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The client part of the generator (README checks 6 to 12) on small fixture directories in a
// temporary directory, with the stored client evidence of the repository.

import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { CLIENT_FLOOR, run } from '../gen.mjs';
import { REPO_ROOT } from '../src/fixture.mjs';
import { PAGES, recordedModes, scenarioCase } from './client-helpers.mjs';

const tmp = mkdtempSync(join(tmpdir(), 'legacy-fixtures-client-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
let counter = 0;

function setup() {
  const dir = join(tmp, `case-${counter++}`);
  cpSync(PAGES, join(dir, 'pages'), { recursive: true });
  return dir;
}

function write(dir, value) {
  mkdirSync(join(dir, value.id), { recursive: true });
  writeFileSync(join(dir, value.id, 'case.json'), JSON.stringify(value));
}

function android(id, extra = {}) {
  return { id: `${id}--ANDROID`, kind: 'client', description: 'legacy-android', profile: 'legacy-android', replaces: id, divergences: ['D-25'], sources: ['docs:engineering/protocol/legacy-client-traces#_6-new-findings'], ...extra };
}

function fixture(scenario, mode, extra = {}) {
  return { ...scenarioCase(scenario, mode), rules: ['CL-20', 'CL-22'], ...extra };
}

const gen = (dir, extra = {}) => run({ dir, root: REPO_ROOT, catalog: false, coverage: false, labPages: null, traces: [], check: true, ...extra });
const write_ = (dir, extra = {}) => gen(dir, { check: false, ...extra });

test('writes_and_checks_a_canonical_client_fixture_and_its_android_override', () => {
  const dir = setup();
  write(dir, fixture('CS-04', 'quick'));
  write(dir, android('FX-LEG-CL-CS04-Q'));
  assert.deepEqual(write_(dir).problems, []);
  const r = gen(dir);
  assert.deepEqual(r.problems, []);
  assert.equal(r.count, 2);
  const canonical = JSON.parse(readFileSync(join(dir, 'FX-LEG-CL-CS04-Q', 'expected.json'), 'utf8'));
  assert.deepEqual(Object.keys(canonical), ['id', 'kind', 'profile', 'mode', 'server_profile', 'level', 'client', 'profiles_same']);
  assert.deepEqual(canonical.profiles_same, []);
  const profile = JSON.parse(readFileSync(join(dir, 'FX-LEG-CL-CS04-Q--ANDROID', 'expected.json'), 'utf8'));
  assert.deepEqual(profile.trace, { scenario: 'CS-04', mode: 'quick' });
  assert.equal(profile.profile, 'legacy-android');
});

test('check6_reports_a_stale_client_expected_json', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick'));
  write_(dir);
  writeFileSync(join(dir, 'FX-LEG-CL-CS12-Q', 'expected.json'), '{}\n');
  assert.deepEqual(gen(dir).problems, ['FX-LEG-CL-CS12-Q/expected.json: stale']);
});

test('check6_a_case_error_names_the_member', () => {
  const dir = setup();
  write(dir, { ...fixture('CS-12', 'quick'), extra: true });
  assert.deepEqual(gen(dir).problems, ['FX-LEG-CL-CS12-Q: unknown member case.extra']);
});

test('check7_the_steps_must_equal_the_stored_steps', () => {
  const dir = setup();
  const c = fixture('CS-12', 'quick');
  c.schedule = c.schedule.filter((s) => s.do !== 'get_diff');
  write(dir, c);
  assert.match(gen(dir, { check: false }).problems[0], /check 7: the steps differ from the steps of run 1 of CS-12 quick/);
});

test('check7_u13_profile_fixture_skips_the_legacy_fields_of_the_500_and_passes', () => {
  const dir = setup();
  write(dir, fixture('CS-11', 'standard'));
  write(dir, android('FX-LEG-CL-CS11-S', { divergences: [], not_reproduced: 'U-13' }));
  assert.deepEqual(write_(dir).problems, []);
  assert.deepEqual(gen(dir).problems, []);
});

test('check7_u13_profile_fixture_whose_trace_has_no_u13_field_is_a_problem', () => {
  const dir = setup();
  write(dir, fixture('CS-03', 'standard'));
  write(dir, android('FX-LEG-CL-CS03-S', { not_reproduced: 'U-13' }));
  assert.match(write_(dir).problems.join('\n'), /FX-LEG-CL-CS03-S--ANDROID: check 7: the legacy-android result differs from the trace: U-13: the trace does not show the documented difference/);
});

test('check6_not_reproduced_takes_only_u13', () => {
  const dir = setup();
  write(dir, fixture('CS-11', 'standard'));
  write(dir, android('FX-LEG-CL-CS11-S', { not_reproduced: 'U-11' }));
  assert.ok(write_(dir).problems.includes('FX-LEG-CL-CS11-S--ANDROID: not_reproduced must be one of U-13'));
});

test('check7_dl119_the_tomcat_content_type_equals_the_java_server_model', () => {
  const dir = setup();
  for (const mode of ['quick', 'standard']) write(dir, fixture('CS-08', mode));
  for (const id of ['FX-LEG-CL-CS08-Q', 'FX-LEG-CL-CS08-S']) write(dir, android(id));
  assert.deepEqual(write_(dir).problems, []);
});

test('check7_profile_fixture_must_equal_the_trace', () => {
  const dir = setup();
  write(dir, fixture('CS-11', 'standard'));
  write(dir, android('FX-LEG-CL-CS11-S'));
  assert.match(write_(dir).problems.join('\n'), /FX-LEG-CL-CS11-S--ANDROID: check 7: the legacy-android result differs from the trace/);
});

test('canonical_equal_to_the_trace_lists_legacy_android_in_profiles_same', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'standard'));
  assert.deepEqual(write_(dir).problems, []);
  assert.deepEqual(JSON.parse(readFileSync(join(dir, 'FX-LEG-CL-CS12-S', 'expected.json'), 'utf8')).profiles_same, ['legacy-android']);
});

test('check8_override_not_needed', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'standard'));
  write(dir, android('FX-LEG-CL-CS12-S'));
  assert.deepEqual(write_(dir).problems, ['FX-LEG-CL-CS12-S--ANDROID: OVERRIDE_NOT_NEEDED: the client member equals FX-LEG-CL-CS12-S']);
});

test('check9_override_missing', () => {
  const dir = setup();
  write(dir, fixture('CS-03', 'standard'));
  assert.match(write_(dir).problems[0], /^FX-LEG-CL-CS03-S: OVERRIDE_MISSING for legacy-android: first difference/);
});

test('check10_unknown_rule_and_rule_coverage', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick', { rules: ['CL-99'] }));
  const r = write_(dir, { coverage: true, clientFloor: 1 });
  assert.ok(r.problems.includes('FX-LEG-CL-CS12-Q: rule CL-99 is not in legacy client section 9'));
  assert.ok(r.problems.includes('rule CL-25 is not covered by any client or function fixture'));
  assert.ok(!r.problems.some((p) => /rule CL-01 /.test(p))); // "—" rules need no fixture
});

test('check11_stored_runs_must_be_used_when_the_client_floor_is_above_0', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick'));
  const r = write_(dir, { coverage: true, clientFloor: 1 });
  assert.ok(r.problems.includes('evidence/traces/client/android/CS-01/quick: stored client runs are not used by any client fixture'));
  assert.ok(!r.problems.some((p) => p.includes('CS-12/quick')));
  assert.ok(r.notes.some((n) => /CS-19 quick: stored runs not used: WebView smoke run/.test(n)));
});

test('check11_one_stored_mode_is_the_trace_of_one_fixture_only', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick'));
  write(dir, { ...fixture('CS-12', 'quick'), id: 'FX-LEG-CL-AGAIN-Q' });
  assert.ok(write_(dir).problems.includes('FX-LEG-CL-CS12-Q: the stored runs of CS-12 quick are already the trace of FX-LEG-CL-AGAIN-Q'));
});

test('check11_without_client_fixtures_is_a_note', () => {
  const dir = setup();
  const r = gen(dir, { coverage: true });
  assert.deepEqual(r.problems, []);
  const unused = recordedModes().length;
  assert.ok(r.notes.includes(`stored-run use is checked when CLIENT_FLOOR is above 0; ${unused} stored scenario modes have no client fixture yet`), r.notes.join('\n'));
});

test('client_floor_is_0_until_the_client_fixtures_exist', () => {
  assert.equal(CLIENT_FLOOR, 0);
});

test('client_floor_0_makes_checks_10_and_11_notes_also_with_a_client_fixture', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick'));
  const r = write_(dir, { coverage: true, clientFloor: 0 });
  assert.deepEqual(r.problems, []);
  assert.ok(r.notes.some((n) => n.startsWith('client rule coverage is checked when CLIENT_FLOOR is above 0; not covered yet: ')));
  assert.ok(r.notes.some((n) => /^stored-run use is checked when CLIENT_FLOOR is above 0; \d+ stored scenario modes/.test(n)));
});

test('client_floor_above_the_number_of_canonical_client_fixtures_is_a_problem', () => {
  const dir = setup();
  write(dir, fixture('CS-12', 'quick'));
  write(dir, fixture('CS-12', 'standard'));
  const at = write_(dir, { coverage: true, clientFloor: 2 });
  assert.ok(!at.problems.some((p) => p.includes('CLIENT_FLOOR')));
  const over = gen(dir, { coverage: true, clientFloor: 3 });
  assert.ok(over.problems.includes('the catalog has 2 canonical client fixtures, fewer than 3 (CLIENT_FLOOR)'));
  // Only the catalog checks it: without coverage (another --dir) the floor does not apply.
  assert.ok(!gen(dir, { coverage: false, clientFloor: 3 }).problems.some((p) => p.includes('CLIENT_FLOOR')));
});

test('check12_function_files_are_generated_and_cases_json_is_a_byte_copy', () => {
  const dir = setup();
  mkdirSync(join(dir, 'functions'));
  assert.deepEqual(write_(dir).problems, []);
  assert.ok(readFileSync(join(dir, 'functions', 'cases.json')).equals(readFileSync(join(REPO_ROOT, 'tools/upstream-lab/client/functions.json'))));
  assert.deepEqual(gen(dir).problems, []);
  writeFileSync(join(dir, 'functions', 'cases.json'), '[]\n');
  writeFileSync(join(dir, 'functions', 'extra.json'), '{}\n');
  const r = gen(dir);
  assert.ok(r.problems.includes('functions/extra.json: unknown file'));
  assert.ok(r.problems.some((p) => p.startsWith('functions/cases.json: stale')));
});

test('a_profile_fixture_must_replace_a_trace_backed_canonical_fixture', () => {
  const dir = setup();
  write(dir, android('FX-LEG-CL-NONE-Q'));
  assert.deepEqual(write_(dir).problems, ['FX-LEG-CL-NONE-Q--ANDROID: replaces FX-LEG-CL-NONE-Q, which is not a trace-backed canonical client fixture']);
});

test('a_model_error_of_a_fixture_is_a_problem', () => {
  const dir = setup();
  write(dir, { ...fixture('CS-01', 'quick'), evidence: 'code-reading', trace: undefined, responses: { r1: { page: 'nosuch' } } });
  assert.match(write_(dir).problems[0], /^FX-LEG-CL-CS01-Q: CASE_PAGE: page nosuch is missing/);
});
