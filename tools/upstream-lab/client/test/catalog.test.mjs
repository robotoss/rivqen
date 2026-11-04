// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { CONTROL_URL, clearArgs, startArgs, stopArgs } from '../src/adb.mjs';
import { buildCaseBody, postCase } from '../src/lab.mjs';
import { FIRST_LOAD, catalogProblems, expandSetup, scenarioProblems, stepProblem } from '../src/steps.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const load = (name) => JSON.parse(readFileSync(path.join(here, '..', name), 'utf8'));
const scenarios = load('scenarios.json');
const functions = load('functions.json');

test('scenarios_json_is_valid_and_has_the_catalog_of_the_sprint_record', () => {
  assert.deepEqual(catalogProblems(scenarios), []);
  assert.deepEqual(
    scenarios.map((s) => s.id),
    ['CS-01', 'CS-02', 'CS-03', 'CS-04', 'CS-05', 'CS-06', 'CS-07', 'CS-08', 'CS-09', 'CS-10', 'CS-11', 'CS-12', 'CS-13a', 'CS-13b', 'CS-14', 'CS-15', 'CS-16', 'CS-17', 'CS-18', 'CS-19'],
  );
  for (const s of scenarios) {
    assert.deepEqual(s.modes, ['quick', 'standard'], s.id);
    assert.equal(s.repeat, s.id === 'CS-19' ? 1 : 3, s.id);
    assert.ok(existsSync(path.join(here, '..', '..', 'pages', `${s.case.page}.html`)), `${s.id} page`);
  }
});

test('scenarios_json_uses_synthetic_accounts_and_no_real_hosts', () => {
  const text = readFileSync(path.join(here, '..', 'scenarios.json'), 'utf8');
  assert.doesNotMatch(text, /https?:\/\//);
  for (const s of scenarios) assert.match(s.config.account, /^rq-test-account(-2)?$/);
});

test('scenarios_json_has_a_hold_before_each_release', () => {
  for (const s of scenarios) {
    s.steps.forEach((step, i) => {
      if (step.do === 'release') assert.ok(s.steps.slice(0, i).some((p) => p.do === 'hold'), `${s.id}: release without hold`);
    });
  }
});

test('functions_json_covers_the_divergences_of_the_brief', () => {
  const need = ['D-20', 'D-21', 'D-22', 'D-23', 'D-24', 'D-25', 'D-26', 'D-28', 'D-29', 'D-30', 'D-31', 'D-32', 'D-33', 'D-34', 'D-35', 'D-39', 'D-44'];
  const have = new Set(functions.flatMap((c) => c.divergences));
  assert.deepEqual(need.filter((d) => !have.has(d)), []);
  const ids = functions.map((c) => c.id);
  assert.equal(new Set(ids).size, ids.length);
  for (const c of functions) assert.ok(['session_id', 'split', 'diff', 'message', 'is_legacy_response', 'template_change', 'etag', 'charset'].includes(c.fn), c.id);
});

test('functions_json_inputs_have_the_members_of_the_contract', () => {
  const members = {
    session_id: ['url', 'account', 'account_related'], split: ['html'], diff: ['local', 'server'], message: ['src_code', 'final_code', 'diff', 'fields'],
    is_legacy_response: ['fields'], template_change: ['value'], etag: ['value'], charset: ['content_type'],
  };
  for (const c of functions) assert.deepEqual(Object.keys(c.input), members[c.fn], c.id);
});

test('functions_json_has_the_edge_cases_of_the_archaeology', () => {
  const inputs = (fn) => functions.filter((c) => c.fn === fn).map((c) => c.input);
  const urls = inputs('session_id').map((i) => i.url);
  const has = (re) => urls.some((u) => re.test(u));
  assert.ok(has(/:8080\//), 'port');
  assert.ok(has(/Shop\.Example\.COM/), 'host case');
  assert.ok(has(/^https:/) && has(/^http:/), 'scheme');
  assert.ok(has(/sonic_a=1&sonic_a=2/), 'repeated name');
  assert.ok(has(/%41/), 'percent-encoding');
  assert.ok(has(/sonic_remain_params=&/), 'empty sonic_remain_params');
  assert.ok(has(/sonic_remain_params=sonic_a;x/), 'name both sonic_ and listed');
  assert.ok(urls.some((u) => !u.includes('sonic_')), 'no sonic_');
  assert.ok(inputs('session_id').some((i) => i.account === '' || i.account === null), 'no account');
  const htmls = inputs('split').map((i) => i.html);
  assert.ok(htmls.some((h) => h.includes('<!--SONICDIFF-')), 'marker case');
  assert.ok(htmls.some((h) => h.includes('<!--sonicdiff-a-->A<!--sonicdiff-b-end-->')), 'opening vs closing name');
  assert.ok(htmls.some((h) => h.includes('<title>')), 'title');
  const diffs = inputs('diff');
  assert.ok(diffs.some((d) => d.server['{b}'] === '' && !(('{b}') in d.local)), 'new key with empty value');
  assert.ok(diffs.some((d) => d.local && JSON.stringify(d.local) === JSON.stringify(d.server)), 'empty diff');
  assert.ok(diffs.some((d) => d.local === null), 'no local data');
});

test('scenario_problems_reports_each_kind_of_error', () => {
  const ok = scenarios[0];
  assert.deepEqual(scenarioProblems(ok), []);
  const bad = (patch) => scenarioProblems({ ...ok, ...patch });
  assert.ok(bad({ id: 'X' }).some((p) => p.includes('id must be')));
  assert.ok(bad({ id: 'CS-1' }).length > 0);
  assert.ok(bad({ id: 'CS-13a' }).length === 0);
  assert.ok(bad({ title: '' }).length > 0);
  assert.ok(bad({ modes: [] }).length > 0);
  assert.ok(bad({ modes: ['slow'] }).length > 0);
  assert.ok(bad({ origin: 'prod' }).length > 0);
  assert.ok(bad({ rows: 'Q1' }).length > 0);
  assert.ok(bad({ repeat: 0 }).length > 0);
  assert.ok(bad({ config: [] }).length > 0);
  assert.ok(bad({ steps: [] }).length > 0);
  assert.ok(bad({ steps: [{ do: 'fly' }] }).some((p) => p.includes('unknown step')));
  assert.ok(bad({ setup: 'first_load' }).length > 0);
  assert.ok(bad({ setup: [{ do: 'fly' }] }).length > 0);
  assert.ok(bad({ setup: [{ case: { page: '../x' } }] }).length > 0);
  assert.ok(bad({ case: { page: 'a', status: 99 } }).length > 0);
  assert.ok(bad({ case: { page: 'a', headers: [['x']] } }).length > 0);
  assert.ok(bad({ case: { page: 'a', other: 1 } }).length > 0);
  assert.ok(bad({ case: null }).length > 0);
  assert.deepEqual(scenarioProblems(null), ['scenario must be an object']);
  assert.ok(catalogProblems([ok, ok]).some((p) => p.includes('duplicate')));
  assert.deepEqual(catalogProblems({}), ['scenario file must be an array']);
});

test('step_problem_checks_the_members_of_each_step', () => {
  const good = [
    { do: 'hold', point: 'head' }, { do: 'hold', point: 'body', bytes: 0 }, { do: 'cut', bytes: 10 }, { do: 'drop_field', name: 'etag' },
    { do: 'wait', for: 'proxy.hold', timeout_ms: 5 }, { do: 'settle', ms: 60000 }, { do: 'set_network', valid: false }, { do: 'set_account', value: '' },
    { do: 'set_cookie', value: 'a=b' }, { do: 'release' },
  ];
  for (const s of good) assert.equal(stepProblem(s), null, JSON.stringify(s));
  const bad = [
    null, [], { do: 'hold', point: 'tail' }, { do: 'hold', point: 'body' }, { do: 'hold', point: 'head', bytes: 1 }, { do: 'cut', bytes: -1 }, { do: 'cut', bytes: 1.5 },
    { do: 'drop_field', name: '' }, { do: 'wait' }, { do: 'wait', for: 'x', timeout_ms: 0 }, { do: 'settle', ms: 60001 }, { do: 'set_network', valid: 'no' },
    { do: 'set_account' }, { do: 'release', extra: 1 }, { do: 'toString' }, { do: 'create_session', x: 1 },
  ];
  for (const s of bad) assert.notEqual(stepProblem(s), null, JSON.stringify(s));
});

test('expand_setup_turns_first_load_into_its_steps', () => {
  const items = expandSetup([{ case: { page: 'a' } }, 'first_load', { do: 'set_account', value: 'x' }]);
  assert.deepEqual(items[0], { case: { page: 'a' } });
  assert.deepEqual(items.slice(1, 1 + FIRST_LOAD.length).map((i) => i.step), FIRST_LOAD);
  assert.deepEqual(items.at(-1), { step: { do: 'set_account', value: 'x' } });
  assert.deepEqual(FIRST_LOAD.map((s) => s.do), ['create_session', 'bind_client', 'client_ready', 'request_resource', 'wait', 'settle', 'destroy']);
  assert.equal(FIRST_LOAD[5].ms, 2500);
});

test('adb_commands_are_built_from_the_configuration', () => {
  assert.deepEqual(clearArgs({}), ['shell', 'pm', 'clear', 'org.rivqen.lab.harness']);
  assert.deepEqual(stopArgs({ package: 'a.b', serial: 'emulator-5554' }), ['-s', 'emulator-5554', 'shell', 'am', 'force-stop', 'a.b']);
  assert.deepEqual(startArgs({}, 'CS-05-quick-r1'), [
    'shell', 'am', 'start', '-W', '-n', 'org.rivqen.lab.harness/.MainActivity', '--es', 'run', 'CS-05-quick-r1', '--es', 'control', CONTROL_URL,
  ]);
  assert.deepEqual(
    startArgs({ package: 'p.q', startCommand: 'shell am instrument -w -e run {run} -e control {control} {package}/{component}', component: 'x.Runner' }, 'r1'),
    ['shell', 'am', 'instrument', '-w', '-e', 'run', 'r1', '-e', 'control', 'http://10.0.2.2:18091', 'p.q/x.Runner'],
  );
});

test('adb_refuses_values_that_the_remote_shell_could_interpret', () => {
  for (const bad of ['a b', 'a;rm', '$(x)', '', 'a`b`', 'a&b', "a'b"]) {
    assert.throws(() => clearArgs({ package: bad }), (e) => e.code === 'BAD_ARG', bad);
    assert.throws(() => startArgs({}, bad), (e) => e.code === 'BAD_ARG', bad);
  }
  assert.throws(() => stopArgs({ serial: 'x;y' }), (e) => e.code === 'BAD_ARG');
  assert.throws(() => startArgs({ component: 'a b' }, 'r'), (e) => e.code === 'BAD_ARG');
});

test('lab_case_body_follows_the_conformance_contract', () => {
  const body = JSON.parse(buildCaseBody({ caseId: 'CS-05-quick-r1', origin: 'lab-php', scenarioCase: { page: 'b' }, pageBytes: Buffer.from('<p>x</p>') }));
  assert.deepEqual(body, {
    suite: 'legacy', case_id: 'CS-05-quick-r1', profile: 'legacy-php', page_base64: Buffer.from('<p>x</p>').toString('base64'),
    app: { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] }, server_config: {},
  });
  const custom = JSON.parse(buildCaseBody({ caseId: 'c', origin: 'lab-java', scenarioCase: { page: 'a', status: 500, headers: [['x', 'y']] }, pageBytes: Buffer.alloc(0) }));
  assert.deepEqual([custom.profile, custom.app], ['legacy-java', { status: 500, headers: [['x', 'y']] }]);
  assert.throws(() => buildCaseBody({ caseId: 'c', origin: 'prod', scenarioCase: { page: 'a' }, pageBytes: Buffer.alloc(0) }), (e) => e.code === 'BAD_ORIGIN');
});

test('lab_post_case_rejects_bad_pages_origins_and_answers', async () => {
  const fetchOk = async () => new Response(null, { status: 204 });
  const base = { origin: 'lab-node', caseId: 'c', scenarioCase: { page: 'a' }, fetchImpl: fetchOk };
  await postCase(base);
  await assert.rejects(postCase({ ...base, scenarioCase: { page: '../a' } }), (e) => e.code === 'BAD_PAGE');
  await assert.rejects(postCase({ ...base, scenarioCase: { page: 'missing' } }), (e) => e.code === 'NO_PAGE');
  await assert.rejects(postCase({ ...base, origin: 'prod' }), (e) => e.code === 'BAD_ORIGIN');
  await assert.rejects(postCase({ ...base, fetchImpl: async () => new Response(null, { status: 422 }) }), (e) => e.code === 'CASE_REJECTED');
  await assert.rejects(postCase({ ...base, fetchImpl: async () => { throw new Error('refused'); } }), (e) => e.code === 'LAB_DOWN');
  let seen;
  await postCase({ ...base, fetchImpl: async (url, init) => { seen = { url, init }; return new Response(null, { status: 204 }); } });
  assert.equal(seen.url, 'http://127.0.0.1:18081/__rivqen/conformance/case');
  assert.equal(seen.init.headers['content-type'], 'application/json');
});
