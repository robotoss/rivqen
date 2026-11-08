// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The client part of the generator (README "Generator checks of the client part") with its own
// evidence, client page and lab files in a temporary directory: how it finds client fixtures,
// rules and stored runs, and the problem that each defect of these inputs gives. Problems are
// the user interface of gen.mjs, so the tests compare the problem lines.

import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { run } from '../gen.mjs';
import { clientPart, clientRules, isClientDir, storedModes } from '../src/client-fixture.mjs';
import { FUNCTION_PROFILES } from '../src/functions.mjs';
import { REPO_ROOT } from '../src/fixture.mjs';
import { PAGES, scenarioCase } from './client-helpers.mjs';

const tmp = mkdtempSync(join(tmpdir(), 'legacy-fixtures-client-part-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
let counter = 0;
const fresh = () => {
  const dir = join(tmp, `t-${counter++}`);
  mkdirSync(dir, { recursive: true });
  return dir;
};
const EVIDENCE = join(REPO_ROOT, 'evidence/traces/client');

function fixtures() {
  const dir = fresh();
  cpSync(PAGES, join(dir, 'pages'), { recursive: true });
  return dir;
}
function write(dir, value) {
  mkdirSync(join(dir, value.id), { recursive: true });
  writeFileSync(join(dir, value.id, 'case.json'), JSON.stringify(value));
}
const fixture = (scenario, mode, extra = {}) => ({ ...scenarioCase(scenario, mode), rules: ['CL-20', 'CL-22'], ...extra });
const android = (id, extra = {}) => ({ id: `${id}--ANDROID`, kind: 'client', description: 'legacy-android', profile: 'legacy-android', replaces: id, divergences: ['D-25'], sources: ['docs:x'], ...extra });
const gen = (dir, extra = {}) => run({ dir, root: REPO_ROOT, catalog: false, coverage: false, labPages: null, traces: [], check: true, clientFloor: 0, ...extra });
const write_ = (dir, extra = {}) => gen(dir, { check: false, ...extra });

/** An evidence directory with the stored runs of one scenario mode: { 'run-1.jsonl': bytes, ... }. */
function evidence(scenario, mode, files) {
  const dir = fresh();
  const runs = join(dir, 'android', scenario, mode);
  mkdirSync(runs, { recursive: true });
  for (const [name, bytes] of Object.entries(files)) writeFileSync(join(runs, name), bytes);
  return dir;
}
const storedRun = (scenario, mode, k = 1) => readFileSync(join(EVIDENCE, 'android', scenario, mode, `run-${k}.jsonl`));

// --- what the client part reads ---

test('a_client_dir_has_a_json_case_with_kind_client', () => {
  const dir = fresh();
  const put = (name, text) => {
    mkdirSync(join(dir, name));
    if (text !== null) writeFileSync(join(dir, name, 'case.json'), text);
  };
  put('client', '{"kind":"client"}');
  put('server', '{"kind":"canonical"}');
  put('text', 'kind: client');
  put('null', 'null');
  put('none', null);
  assert.equal(isClientDir(dir, 'client'), true);
  for (const name of ['server', 'text', 'null', 'none']) assert.equal(isClientDir(dir, name), false, name);
});

test('client_rules_reads_the_rule_rows_and_their_last_column', () => {
  const page = join(fresh(), 'legacy-client.md');
  writeFileSync(page, [
    '| CL-01 | a | — |',
    '| CL-02 | b | Fixture |   ',
    '| CL-03 | c |  Fixture  |',
    'x| CL-04 | d | Fixture |',
    '| CL-05 | e | Fixture | tail',
    '| CL-6 | f | Fixture |',
    '',
  ].join('\n'));
  assert.deepEqual([...clientRules(page)], [['CL-01', false], ['CL-02', true], ['CL-03', true]]);
});

test('stored_modes_are_the_mode_directories_of_the_scenario_directories', () => {
  const dir = fresh();
  for (const path of ['CS-02/standard', 'CS-01/quick', 'CS-01/standard', 'CS-03a/quick', 'notes/quick', 'xCS-04/quick', 'CS-05ab/quick', 'CS-07/other']) mkdirSync(join(dir, 'android', path), { recursive: true });
  writeFileSync(join(dir, 'android', 'CS-06'), '');
  assert.deepEqual(storedModes(dir).map((m) => `${m.scenario} ${m.mode}`), ['CS-01 quick', 'CS-01 standard', 'CS-02 standard', 'CS-03a quick']);
  assert.equal(storedModes(dir)[0].dir, join(dir, 'android', 'CS-01', 'quick'));
  assert.deepEqual(storedModes(join(dir, 'none')), []);
});

// --- check 7: the stored runs ---

test('check7_a_stored_mode_without_runs_is_a_problem_of_both_fixtures', () => {
  const dir = fixtures();
  write(dir, fixture('CS-04', 'quick'));
  write(dir, android('FX-LEG-CL-CS04-Q'));
  // Only run-<k>.jsonl files are runs.
  const clientEvidence = evidence('CS-04', 'quick', { 'run-x.jsonl': 'x', 'xrun-1.jsonl': 'x', 'run-1.jsonl.bak': 'x', 'run-.jsonl': 'x' });
  const problems = write_(dir, { clientEvidence }).problems;
  assert.ok(problems.includes('FX-LEG-CL-CS04-Q: check 7: no stored run for CS-04 quick'), problems.join('\n'));
  assert.ok(problems.includes('FX-LEG-CL-CS04-Q--ANDROID: check 7: no stored run for CS-04 quick'), problems.join('\n'));
});

test('check7_stored_runs_are_read_in_run_number_order', () => {
  const dir = fixtures();
  write(dir, fixture('CS-12', 'quick'));
  const good = storedRun('CS-12', 'quick');
  const fewer = Buffer.from(good.toString('utf8').split('\n').filter((l) => !l.includes('"do":"get_diff"')).join('\n'));
  // Run 10 is the third run: it is after run 2 and run 3 in number order, before them in text order.
  const clientEvidence = evidence('CS-12', 'quick', { 'run-3.jsonl': good, 'run-10.jsonl': fewer, 'run-2.jsonl': good });
  assert.deepEqual(write_(dir, { clientEvidence }).problems, ['FX-LEG-CL-CS12-Q: check 7: the steps differ from the steps of run 3 of CS-12 quick']);
});

test('check7_an_io_error_of_a_stored_run_propagates', () => {
  const dir = fixtures();
  write(dir, fixture('CS-12', 'quick'));
  const clientEvidence = evidence('CS-12', 'quick', {});
  mkdirSync(join(clientEvidence, 'android', 'CS-12', 'quick', 'run-1.jsonl'));
  assert.throws(() => write_(dir, { clientEvidence }), { code: 'EISDIR' });
});

test('check7_a_stored_run_that_is_not_a_trace_is_a_problem', () => {
  const dir = fixtures();
  write(dir, fixture('CS-12', 'quick'));
  const clientEvidence = evidence('CS-12', 'quick', { 'run-1.jsonl': 'not json\n' });
  assert.deepEqual(write_(dir, { clientEvidence }).problems, ['FX-LEG-CL-CS12-Q: check 7: a stored run of CS-12 quick: line 1 is not JSON']);
});

// --- checks 6, 8 to 11 ---

test('a_client_fixture_directory_with_another_file_is_a_problem', () => {
  const dir = fixtures();
  write(dir, fixture('CS-12', 'standard'));
  writeFileSync(join(dir, 'FX-LEG-CL-CS12-S', 'notes.txt'), 'x');
  assert.deepEqual(write_(dir).problems, ['FX-LEG-CL-CS12-S/notes.txt: unknown file']);
});

test('a_case_json_that_is_not_json_is_a_problem_of_the_client_part', () => {
  // gen.mjs passes only directories whose case.json parses; clientPart still reports, not throws.
  const dir = fresh();
  mkdirSync(join(dir, 'FX-LEG-CL-X-Q'));
  writeFileSync(join(dir, 'FX-LEG-CL-X-Q', 'case.json'), '{');
  const part = (names) => clientPart({ dir, names, page: () => null, serverFixture: () => null, evidenceDir: fresh(), clientPage: join(REPO_ROOT, 'docs/engineering/protocol/legacy-client.md'), labFunctions: '', scenarios: '', coverage: false, floor: 0, sync: () => null });
  assert.deepEqual(part(['FX-LEG-CL-X-Q']).problems, ['FX-LEG-CL-X-Q: case.json is not JSON']);
  // An I/O error (here: no case.json) is not a case defect: it propagates.
  assert.throws(() => part(['FX-LEG-CL-NONE-Q']), { code: 'ENOENT' });
});

test('a_profile_fixture_cannot_replace_a_code_reading_fixture', () => {
  const dir = fixtures();
  write(dir, { ...fixture('CS-12', 'quick'), evidence: 'code-reading', trace: undefined });
  write(dir, android('FX-LEG-CL-CS12-Q'));
  assert.deepEqual(write_(dir).problems, ['FX-LEG-CL-CS12-Q--ANDROID: replaces FX-LEG-CL-CS12-Q, which is not a trace-backed canonical client fixture']);
});

test('the_client_page_must_have_rules_when_client_or_function_fixtures_exist', () => {
  const noRules = join(fresh(), 'legacy-client.md');
  writeFileSync(noRules, '# no rules\n');
  const withFixture = fixtures();
  write(withFixture, fixture('CS-12', 'standard'));
  const problems = write_(withFixture, { clientPage: noRules }).problems;
  assert.ok(problems.includes(`${noRules}: no rule CL-nn found`), problems.join('\n'));
  assert.ok(problems.includes('FX-LEG-CL-CS12-S: rule CL-20 is not in legacy client section 9'), problems.join('\n'));
  const withFunctions = fixtures();
  mkdirSync(join(withFunctions, 'functions'));
  assert.deepEqual(write_(withFunctions, { clientPage: noRules }).problems, [`${noRules}: no rule CL-nn found`]);
  assert.deepEqual(write_(fixtures(), { clientPage: noRules }).problems, []);
});

test('the_floor_counts_canonical_client_fixtures_only', () => {
  const dir = fixtures();
  write(dir, fixture('CS-04', 'quick'));
  write(dir, android('FX-LEG-CL-CS04-Q'));
  const r = write_(dir, { coverage: true, clientFloor: 2 });
  assert.ok(r.problems.includes('the catalog has 1 canonical client fixtures, fewer than 2 (CLIENT_FLOOR)'), r.problems.join('\n'));
});

// --- check 12: the function fixtures ---

test('the_count_includes_one_file_per_function_profile', () => {
  const dir = fixtures();
  mkdirSync(join(dir, 'functions'));
  assert.equal(write_(dir).count, FUNCTION_PROFILES.length);
  write(dir, fixture('CS-12', 'standard'));
  assert.equal(write_(dir).count, FUNCTION_PROFILES.length + 1);
});

test('without_function_fixtures_the_catalog_gets_a_note', () => {
  const r = write_(fixtures(), { coverage: true });
  assert.ok(r.notes.includes('functions: no function fixtures yet (check 12 skipped)'), r.notes.join('\n'));
});

test('function_fixtures_cover_their_rules', () => {
  const uncovered = (dir) => write_(dir, { coverage: true, clientFloor: 1 }).problems.filter((p) => /^rule CL-\d\d is not covered/.test(p)).map((p) => p.slice(5, 10));
  const withFunctions = fixtures();
  write(withFunctions, fixture('CS-12', 'quick'));
  mkdirSync(join(withFunctions, 'functions'));
  const without = fixtures();
  write(without, fixture('CS-12', 'quick'));
  const covered = ['CL-14', 'CL-15', 'CL-16', 'CL-23', 'CL-24', 'CL-28', 'CL-41', 'CL-42', 'CL-43'];
  for (const rule of covered) assert.ok(uncovered(without).includes(rule), rule);
  assert.deepEqual(uncovered(withFunctions).filter((r) => covered.includes(r)), []);
});

test('a_lab_case_file_that_does_not_parse_is_a_problem', () => {
  const dir = fixtures();
  mkdirSync(join(dir, 'functions'));
  const labFunctions = join(fresh(), 'functions.json');
  writeFileSync(labFunctions, '{}\n');
  const r = write_(dir, { labFunctions });
  assert.deepEqual(r.problems, ['functions/cases.json: the case file must be an array of at most 4096 cases']);
  assert.equal(r.count, 0);
});

test('a_function_evidence_file_that_does_not_parse_is_a_problem', () => {
  const dir = fixtures();
  mkdirSync(join(dir, 'functions'));
  const clientEvidence = fresh();
  mkdirSync(join(clientEvidence, 'android'));
  writeFileSync(join(clientEvidence, 'android', 'functions.jsonl'), 'not json\n');
  // The iOS files are missing: they are skipped, not an I/O error.
  const problems = write_(dir, { clientEvidence }).problems;
  assert.equal(problems[0], 'evidence/traces/client/android/functions.jsonl: line 1 is not JSON');
  assert.ok(problems.includes('functions/legacy-android.json: FN-ID-01: no evidence line'), problems.slice(0, 3).join('\n'));
});

test('a_stale_function_file_is_a_problem', () => {
  const dir = fixtures();
  mkdirSync(join(dir, 'functions'));
  assert.deepEqual(write_(dir).problems, []);
  writeFileSync(join(dir, 'functions', 'legacy-ios.json'), '{}\n');
  assert.deepEqual(gen(dir).problems, ['functions/legacy-ios.json: stale']);
});

// --- responses that name a server fixture of the directory ---

test('a_fixture_response_takes_the_page_and_app_of_a_canonical_server_fixture_of_the_directory', () => {
  const dir = fixtures();
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['x-app', '1']] };
  mkdirSync(join(dir, 'FX-LEG-SRV'));
  writeFileSync(join(dir, 'FX-LEG-SRV', 'case.json'), JSON.stringify({
    id: 'FX-LEG-SRV', description: 'test', profile: 'legacy-canonical', rules: ['C-02'], upstream: scenarioCase('CS-12', 'quick').upstream,
    sources: ['docs:engineering/protocol/legacy-wire#canonical-server'], evidence: 'code-reading', page: 'b', app, request: { headers: [['accept-diff', 'true']] },
  }));
  const schedule = [{ case: 'r1' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'settle', ms: 2000 }, { do: 'snapshot_cache' }];
  const client = (id, responses) => ({ ...fixture('CS-12', 'quick'), id, evidence: 'code-reading', trace: undefined, responses: { r1: responses }, schedule });
  write(dir, client('FX-LEG-CL-BYFIXTURE-Q', { fixture: 'FX-LEG-SRV' }));
  write(dir, client('FX-LEG-CL-BYPAGE-Q', { page: 'b', app }));
  assert.deepEqual(write_(dir).problems, []);
  const expected = (id) => JSON.parse(readFileSync(join(dir, id, 'expected.json'), 'utf8')).client;
  assert.deepEqual(expected('FX-LEG-CL-BYFIXTURE-Q'), expected('FX-LEG-CL-BYPAGE-Q'));
  write(dir, client('FX-LEG-CL-NOFIXTURE-Q', { fixture: 'FX-LEG-NONE' }));
  assert.ok(write_(dir).problems.includes('FX-LEG-CL-NOFIXTURE-Q: CASE_FIXTURE: responses: server fixture FX-LEG-NONE is not a canonical server fixture'));
});
