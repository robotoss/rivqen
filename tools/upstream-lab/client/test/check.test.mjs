// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { canonicalLine } from '../src/canon.mjs';
import { checkRunText, checkTree, eventProblems, foreignHosts, functionsProblems, runJsonProblems } from '../src/check.mjs';
import { firstDifference, normalizeEvent, normalizeRun } from '../src/determinism.mjs';
import { buildRunJson } from '../src/runjson.mjs';

const SHA = 'a'.repeat(64);
const HTTP = 'GET /catalog HTTP/1.1\nHost: 10.0.2.2:18090\n\n--- response ---\nHTTP 200\nDate: Thu, 05 Nov 2026 10:00:00 GMT\n\n[body decoded from content-encoding=identity]\n<p/>';

/** Events of a valid run. `date` and `worker` change values that the determinism rule ignores. */
function runEvents({ date = 'Thu, 05 Nov 2026 10:00:00 GMT', worker = 'worker-1', t0 = 0, extra = [], ok = true } = {}) {
  const raw = [
    ['conductor', 'run.start', { scenario: 'CS-01', mode: 'quick', repeat: 1, origin: 'lab-node', upstream_sha: '59936bef' }],
    ['conductor', 'step.start', { step: { do: 'create_session', seq: 1 } }],
    ['harness', 'session.created', { thread: 'main', session_id: 'abc' }],
    ['harness', 'step.done', { thread: 'main', step: { do: 'create_session', seq: 1 } }],
    ['proxy', 'proxy.request', { exchange: 1, method: 'GET', target: '/catalog', fields: [['Host', '10.0.2.2:18090']] }],
    ['proxy', 'proxy.response_head', { exchange: 1, status: 200, fields: [['Date', date], ['ETag', 'e1']] }],
    ['proxy', 'proxy.response_end', { exchange: 1, body_len: 4, body_sha256: SHA, complete: true }],
    ['harness', 'client.intercept', { thread: worker, status: 200, reason: 'OK', mime: 'text/html', encoding: 'utf-8', headers: [['content-type', 'text/html']], body_len: 4, body_sha256: SHA, complete: true }],
    ['harness', 'cache.snapshot', { thread: 'main', files: [{ name: 'a', size: 1, sha256: SHA }, { name: 'b', size: 2, sha256: SHA }], db: null }],
    ...extra,
    ['conductor', 'run.end', { ok, error: ok ? null : 'x' }],
  ];
  return raw.map(([src, ev, members], i) => ({ ...members, ev, seq: i + 1, src, t: t0 + i }));
}
const toText = (events) => events.map((e) => `${canonicalLine(e)}\n`).join('');

async function tree(runs, { runJson = 'auto', files = 'auto', scenario = 'CS-01', mode = 'quick', extraFiles = {} } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'lab-check-'));
  const dir = path.join(root, 'android', scenario, mode);
  await mkdir(dir, { recursive: true });
  for (const [i, text] of runs.entries()) {
    await writeFile(path.join(dir, `run-${i + 1}.jsonl`), text);
    if (files === 'auto') await writeFile(path.join(dir, `run-${i + 1}-x1.http`), HTTP);
  }
  for (const [name, text] of Object.entries(extraFiles)) await writeFile(path.join(dir, name), text);
  if (runJson === 'auto') {
    const j = await buildRunJson({ root, meta: { date: '2026-11-05', tools: { node: 'v22' }, upstreamSha: '59936bef', emulatorImage: 'api28' } });
    await writeFile(path.join(root, 'run.json'), JSON.stringify(j));
  } else if (runJson !== null) {
    await writeFile(path.join(root, 'run.json'), typeof runJson === 'string' ? runJson : JSON.stringify(runJson));
  }
  return { root, done: () => rm(root, { recursive: true, force: true }) };
}
async function errorsOf(runs, options = {}, checkOptions = {}) {
  const t = await tree(runs, options);
  try {
    return (await checkTree(t.root, checkOptions)).errors;
  } finally {
    await t.done();
  }
}

test('check_accepts_a_valid_tree', async () => {
  assert.deepEqual(await errorsOf([toText(runEvents()), toText(runEvents()), toText(runEvents())]), []);
});

test('check_schema_rejects_bad_events', () => {
  const bad = (patch) => eventProblems({ ...runEvents()[2], ...patch });
  assert.deepEqual(bad({}), []);
  assert.ok(bad({ ev: 'session.nope' }).some((p) => p.startsWith('unknown event')));
  assert.ok(bad({ session_id: 5 }).includes('session.created: bad session_id'));
  const { session_id: _gone, ...without } = runEvents()[2];
  assert.ok(eventProblems(without).includes('session.created: missing session_id'));
  assert.ok(bad({ extra: 1 }).includes('session.created: unknown member extra'));
  assert.ok(bad({ src: 'proxy' }).some((p) => p.includes('must come from harness')));
  assert.ok(bad({ src: 'other' }).length > 0);
  assert.ok(bad({ thread: undefined }).includes('harness event without thread'));
  assert.ok(bad({ seq: 0 }).length > 0);
  assert.ok(bad({ t: -1 }).length > 0);
  assert.ok(bad({ t: 1.5 }).length > 0);
  assert.deepEqual(eventProblems([]), ['not an object']);
  assert.deepEqual(eventProblems({ ev: 3 }), ['ev is not a string']);
  const conductor = runEvents()[0];
  assert.ok(eventProblems({ ...conductor, thread: 'main' }).includes('thread only on harness events'));
  assert.ok(eventProblems({ ...conductor, ev: 'toString' }).some((p) => p.startsWith('unknown event')));
});

test('check_schema_covers_the_members_of_each_event_of_the_contract', () => {
  const base = { seq: 1, t: 0 };
  const h = { ...base, src: 'harness', thread: 'main' };
  const ok = [
    { ...h, ev: 'client.load_url', url: 'http://10.0.2.2:18090/catalog', extras: ['a'] },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'text/html', encoding: 'utf-8', history_url: 'u', headers: [['a', 'b']], body_len: 1, body_sha256: SHA },
    { ...h, ev: 'client.intercept', null: true },
    { ...h, ev: 'client.intercept', status: 200, reason: 'OK', mime: 'm', encoding: 'e', headers: [], body_len: 0, body_sha256: SHA, complete: false, error: 'IOException' },
    { ...h, ev: 'session.result', src_code: 1000, final_code: 304 },
    { ...h, ev: 'session.message', json: '{}' },
    { ...h, ev: 'session.callback', name: 'onSessionHitCache', args: [] },
    { ...h, ev: 'runtime.cookie_get', url: 'u', value: null },
    { ...h, ev: 'runtime.cookie_set', url: 'u', value: 'a=b' },
    { ...base, src: 'proxy', ev: 'proxy.cut', exchange: 1, bytes: 5 },
    { ...base, src: 'proxy', ev: 'proxy.drop_field', exchange: 1, name: 'etag' },
    { ...base, src: 'proxy', ev: 'proxy.hold', exchange: 1, point: 'body', bytes: 3 },
    { ...base, src: 'proxy', ev: 'proxy.release', exchange: null, point: 'head' },
    { ...base, src: 'conductor', ev: 'step.done', step: { do: 'x' }, error: 'boom' },
  ];
  for (const e of ok) assert.deepEqual(eventProblems(e), [], `${e.ev} ${JSON.stringify(e)}`);
  const bad = [
    { ...h, ev: 'client.intercept', null: true, status: 200 },
    { ...h, ev: 'client.intercept', null: false },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'm', encoding: 'e', history_url: 'u', headers: [['a']], body_len: 1, body_sha256: SHA },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'm', encoding: 'e', history_url: 'u', headers: [], body_len: 1, body_sha256: 'xyz' },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'b', size: 1, sha256: SHA }, { name: 'a', size: 1, sha256: SHA }], db: null },
    { ...h, ev: 'cache.snapshot', files: [], db: {} },
    { ...base, src: 'proxy', ev: 'proxy.request', exchange: 0, method: 'GET', target: '/', fields: [] },
    { ...base, src: 'conductor', ev: 'run.end', ok: 'yes', error: null },
  ];
  for (const e of bad) assert.notDeepEqual(eventProblems(e), [], JSON.stringify(e));
});

test('check_sequence_rejects_gaps_a_bad_start_a_bad_end_and_time_going_back', () => {
  const run = runEvents();
  const problems = (events, text = toText(events)) => checkRunText(text).errors;
  assert.deepEqual(problems(run), []);
  const gap = run.map((e, i) => (i === 3 ? { ...e, seq: 9 } : e));
  assert.ok(problems(gap).some((p) => p.includes('seq 9, expected 4')));
  assert.ok(problems(run.slice(1).map((e, i) => ({ ...e, seq: i + 1 }))).includes('the first event is not run.start'));
  assert.ok(problems(run.slice(0, -1)).includes('the last event is not run.end'));
  assert.ok(problems(runEvents({ ok: false })).some((p) => p.startsWith('the run did not succeed: x')));
  const back = run.map((e, i) => ({ ...e, t: 100 - i }));
  assert.ok(problems(back).includes('t goes back'));
  assert.ok(problems(run, toText(run).slice(0, -1)).includes('the last line has no LF'));
  assert.deepEqual(checkRunText('').errors, ['empty event log']);
  assert.ok(checkRunText('not json\n').errors.includes('line 1: not JSON'));
  const twice = [...run.slice(0, -1), { ...run[0], seq: run.length }, run.at(-1)].map((e, i) => ({ ...e, seq: i + 1 }));
  assert.ok(problems(twice).includes('more than one run.start or run.end'));
});

test('check_sequence_rejects_keys_that_are_not_sorted', () => {
  const [first, ...rest] = toText(runEvents()).split('\n');
  const unsorted = JSON.stringify(Object.fromEntries(Object.entries(JSON.parse(first)).reverse()));
  assert.ok(checkRunText([unsorted, ...rest].join('\n')).errors.some((p) => p.includes('keys not sorted')));
});

test('check_sequence_rejects_an_exchange_with_a_missing_part', () => {
  const events = runEvents().filter((e) => e.ev !== 'proxy.response_end').map((e, i) => ({ ...e, seq: i + 1 }));
  assert.ok(checkRunText(toText(events)).errors.includes('exchange 1 has no response_head or response_end'));
  const orphan = runEvents().filter((e) => e.ev !== 'proxy.request').map((e, i) => ({ ...e, seq: i + 1 }));
  assert.ok(checkRunText(toText(orphan)).errors.includes('exchange 1 has no request'));
});

test('check_exchange_files_must_match_the_proxy_events', async () => {
  const t1 = await tree([toText(runEvents())], { files: 'none' });
  try {
    assert.deepEqual((await checkTree(t1.root)).errors, ['CS-01/quick/run-1.jsonl: missing run-1-x1.http']);
  } finally {
    await t1.done();
  }
  const errors = await errorsOf([toText(runEvents())], { extraFiles: { 'run-1-x2.http': HTTP, 'run-1-x3.txt': 'x' } });
  assert.deepEqual(errors.sort(), ['CS-01/quick/run-1-x2.http: no proxy.request for this exchange', 'CS-01/quick/run-1-x3.txt: unexpected file']);
  const bad = await errorsOf([toText(runEvents())], { extraFiles: { 'run-1-x1.http': 'garbage' } });
  assert.deepEqual(bad, ['CS-01/quick/run-1-x1.http: not the .http format']);
});

test('check_determinism_ignores_t_worker_threads_and_date_values', async () => {
  const a = toText(runEvents());
  const b = toText(runEvents({ date: 'Fri, 06 Nov 2026 11:11:11 GMT', worker: 'worker-7', t0: 500 }));
  assert.deepEqual(await errorsOf([a, b, a]), []);
});

test('check_determinism_reports_runs_that_differ', async () => {
  const a = toText(runEvents());
  const other = runEvents().map((e) => (e.ev === 'proxy.response_head' ? { ...e, status: 304 } : e));
  const stable = { date: '2026-11-05', tools: { node: 'v22' }, emulator_image: 'x', upstream_sha: '59936bef', scenarios: [{ id: 'CS-01', modes: { quick: { runs: 3 } } }], summary: { stable: [{ scenario: 'CS-01', mode: 'quick' }], unstable: [] } };
  const errors = await errorsOf([a, a, toText(other)], { runJson: stable });
  assert.deepEqual(errors, [
    'CS-01/quick: runs differ (run 3 at line 6) and run.json gives no reason',
    'CS-01/quick: run.json says stable, the runs differ',
  ]);
});

test('check_determinism_accepts_unstable_runs_with_a_reason_in_run_json', async () => {
  const a = toText(runEvents());
  const other = toText(runEvents().map((e) => (e.ev === 'proxy.response_head' ? { ...e, status: 304 } : e)));
  const t = await tree([a, a, other]);
  try {
    const j = JSON.parse(await (await import('node:fs/promises')).readFile(path.join(t.root, 'run.json'), 'utf8'));
    assert.deepEqual(j.summary.unstable.map((u) => [u.scenario, u.mode]), [['CS-01', 'quick']]);
    j.summary.unstable[0].reason = 'the order of head and ready varies';
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify(j));
    const res = await checkTree(t.root);
    assert.deepEqual(res.errors, []);
    assert.deepEqual(res.unstable.map((u) => `${u.scenario}/${u.mode}`), ['CS-01/quick']);
    // the same reason for runs that are equal is an error
    j.summary.stable = [];
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify({ ...j, summary: { stable: [{ scenario: 'CS-01', mode: 'quick' }], unstable: j.summary.unstable } }));
    assert.ok((await checkTree(t.root)).errors.some((e) => e.includes('run.json says stable, the runs differ')));
  } finally {
    await t.done();
  }
});

test('check_determinism_rejects_a_reason_for_stable_runs', async () => {
  const a = toText(runEvents());
  const t = await tree([a, a, a]);
  try {
    const j = JSON.parse(await (await import('node:fs/promises')).readFile(path.join(t.root, 'run.json'), 'utf8'));
    j.summary = { stable: [], unstable: [{ scenario: 'CS-01', mode: 'quick', reason: 'why' }] };
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify(j));
    assert.deepEqual((await checkTree(t.root)).errors, ['CS-01/quick: run.json says unstable, the runs are equal', 'CS-01/quick: not listed in run.json summary']);
    j.summary = { stable: [], unstable: [] };
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify(j));
    assert.deepEqual((await checkTree(t.root)).errors, ['CS-01/quick: not listed in run.json summary']);
  } finally {
    await t.done();
  }
});

test('check_run_json_is_required_and_validated', async () => {
  const a = toText(runEvents());
  assert.deepEqual(await errorsOf([a], { runJson: null }), ['run.json is missing']);
  assert.deepEqual(await errorsOf([a], { runJson: '{' }), ['run.json is not valid JSON']);
  const good = { date: '2026-11-05', tools: { node: 'v22' }, emulator_image: 'x', upstream_sha: '59936bef', scenarios: [{ id: 'CS-01', modes: {} }], summary: { stable: [{ scenario: 'CS-01', mode: 'quick' }], unstable: [] } };
  assert.deepEqual(runJsonProblems(good), []);
  const bad = [
    [{ ...good, date: '5 Nov' }, 'date'], [{ ...good, tools: { node: 22 } }, 'tools'], [{ ...good, emulator_image: 1 }, 'emulator_image'],
    [{ ...good, upstream_sha: 'zz' }, 'upstream_sha'], [{ ...good, scenarios: [{ id: 'CS-01' }] }, 'scenarios'], [{ ...good, summary: { stable: [], unstable: [{ scenario: 'a', mode: 'quick', reason: '' }] } }, 'summary'],
    [{ ...good, summary: [] }, 'summary'],
  ];
  for (const [j, key] of bad) assert.ok(runJsonProblems(j).some((p) => p.includes(key)), key);
  assert.deepEqual(runJsonProblems([]), ['run.json is not an object']);
  for (const key of Object.keys(good)) {
    const { [key]: _omitted, ...rest } = good;
    assert.ok(runJsonProblems(rest).includes(`run.json: missing ${key}`));
  }
  const listed = { ...good, scenarios: [{ id: 'CS-02', modes: {} }] };
  const errors = await errorsOf([a], { runJson: listed });
  assert.ok(errors.includes('CS-01: not listed in run.json scenarios'));
  assert.ok(errors.includes('CS-02: listed in run.json but has no runs'));
});

test('check_catalog_requires_the_modes_and_the_repeat_count', async () => {
  const a = toText(runEvents());
  const entry = { id: 'CS-01', modes: ['quick', 'standard'], repeat: 3 };
  const errors = await errorsOf([a, a], {}, { catalog: [entry] });
  assert.ok(errors.includes('CS-01/quick: fewer than 3 runs'));
  assert.ok(errors.includes('CS-01/standard: fewer than 3 runs'));
  const wrongMode = await errorsOf([a], {}, { catalog: [{ ...entry, modes: ['standard'], repeat: 1 }] });
  assert.ok(wrongMode.includes('CS-01/quick: mode is not in scenarios.json'));
  const unknown = await errorsOf([a], {}, { catalog: [{ id: 'CS-02', modes: ['quick'], repeat: 1 }] });
  assert.ok(unknown.includes('CS-01: not in scenarios.json'));
});

test('check_rejects_foreign_hosts_in_logs_and_http_files', async () => {
  assert.deepEqual(foreignHosts('see http://10.0.2.2:18090/x https://shop.example.com/y http://localhost/ http://a.example.org'), []);
  assert.deepEqual(foreignHosts('http://evil.test/x https://real.example.io:8080/'), ['evil.test', 'real.example.io']);
  const leak = runEvents({ extra: [['harness', 'client.load_url', { thread: 'main', url: 'https://www.real-shop.com/p', extras: [] }]] });
  assert.ok((await errorsOf([toText(leak)])).some((e) => e.includes('foreign host in the log: www.real-shop.com')));
  const http = await errorsOf([toText(runEvents())], { extraFiles: { 'run-1-x1.http': `${HTTP}\nhttp://tracker.test/` } });
  assert.deepEqual(http, ['CS-01/quick/run-1-x1.http: foreign host tracker.test']);
});

test('check_rejects_unexpected_entries', async () => {
  const t = await tree([toText(runEvents())]);
  try {
    await mkdir(path.join(t.root, 'android', 'CS-01', 'fast'));
    assert.ok((await checkTree(t.root)).errors.includes('CS-01/fast: unexpected entry'));
  } finally {
    await t.done();
  }
});

test('check_functions_jsonl_needs_one_known_result_per_case', () => {
  const cases = [{ id: 'FN-1' }, { id: 'FN-2' }];
  const line = (o) => `${JSON.stringify(o)}\n`;
  const ok = line({ id: 'FN-1', fn: 'etag', platform: 'ios', result: { value: 'a' } }) + line({ id: 'FN-2', fn: 'etag', platform: 'ios', unsupported: 'no function' });
  assert.deepEqual(functionsProblems(ok, 'ios', cases), []);
  const bad = line({ id: 'FN-1', fn: 'etag', platform: 'android', result: {} }) + line({ id: 'FN-9', fn: 'etag', platform: 'ios', result: {} }) +
    line({ id: 'FN-1', fn: 'etag', platform: 'ios', result: {}, error: 'x' }) + 'nope\n';
  const problems = functionsProblems(bad, 'ios', cases);
  assert.ok(problems.some((p) => p.includes('needs id, fn and platform ios')));
  assert.ok(problems.some((p) => p.includes('unknown case FN-9')));
  assert.ok(problems.some((p) => p.includes('exactly one of result, error, unsupported')));
  assert.ok(problems.some((p) => p.includes('not JSON')));
  assert.ok(problems.includes('case FN-2 has no result'));
  const dup = line({ id: 'FN-1', fn: 'e', platform: 'ios', result: 1 }) + line({ id: 'FN-1', fn: 'e', platform: 'ios', result: 1 });
  assert.ok(functionsProblems(dup, 'ios', cases).some((p) => p.includes('duplicate case FN-1')));
});

test('determinism_normalizes_t_thread_and_date_values_only', () => {
  const e = { ev: 'proxy.response_head', seq: 3, src: 'proxy', t: 99, thread: 'worker-2', fields: [['Date', 'x'], ['ETag', 'e'], ['date', 'y']], extra: { headers: [['DATE', 'z']] } };
  assert.deepEqual(normalizeEvent(e), { ev: 'proxy.response_head', seq: 3, src: 'proxy', fields: [['Date', ''], ['ETag', 'e'], ['date', '']], extra: { headers: [['DATE', '']] } });
  assert.equal(normalizeEvent({ ...e, thread: 'main' }).thread, 'main');
  assert.deepEqual(normalizeEvent({ ev: 'a', args: ['date', 5], other: [['dates', 'keep']] }), { ev: 'a', args: ['date', 5], other: [['dates', 'keep']] });
  assert.deepEqual(normalizeRun([{ ev: 'a', t: 1, z: 1, b: 2 }]), ['{"b":2,"ev":"a","z":1}']);
});

test('determinism_first_difference_names_the_run_and_the_line', () => {
  assert.equal(firstDifference([['a', 'b'], ['a', 'b'], ['a', 'b']]), null);
  assert.deepEqual(firstDifference([['a', 'b'], ['a', 'b'], ['a', 'c']]), { run: 2, index: 1, a: 'b', b: 'c' });
  assert.deepEqual(firstDifference([['a'], ['a', 'b']]), { run: 1, index: 1, a: null, b: 'b' });
  assert.deepEqual(firstDifference([['a', 'b'], ['a']]), { run: 1, index: 1, a: 'b', b: null });
  assert.equal(firstDifference([['a']]), null);
});

test('run_json_is_rebuilt_from_the_tree_and_keeps_recorded_reasons', async () => {
  const a = toText(runEvents());
  const other = toText(runEvents().map((e) => (e.ev === 'proxy.response_head' ? { ...e, status: 304 } : e)));
  const t = await tree([a, other], { runJson: null });
  try {
    const meta = { date: '2026-11-05', tools: { node: 'v22' }, upstreamSha: '59936bef' };
    const first = await buildRunJson({ root: t.root, meta });
    assert.equal(first.emulator_image, 'unknown');
    assert.deepEqual(first.scenarios, [{ id: 'CS-01', modes: { quick: { runs: 2 } } }]);
    assert.deepEqual(first.summary.stable, []);
    assert.deepEqual(first.summary.unstable, [{ scenario: 'CS-01', mode: 'quick', reason: 'runs differ: run 2 at line 6' }]);
    const previous = { emulator_image: 'api28', summary: { unstable: [{ scenario: 'CS-01', mode: 'quick', reason: 'recorded by hand' }] } };
    const second = await buildRunJson({ root: t.root, previous, meta });
    assert.equal(second.summary.unstable[0].reason, 'recorded by hand');
    assert.equal(second.emulator_image, 'api28');
    assert.equal((await buildRunJson({ root: t.root, previous, meta: { ...meta, emulatorImage: 'api29' } })).emulator_image, 'api29');
  } finally {
    await t.done();
  }
});
