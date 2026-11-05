// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { canonicalLine } from '../src/canon.mjs';
import { checkRunText, checkTree, eventProblems, foreignHosts, functionsProblems, metaProblems, runJsonProblems } from '../src/check.mjs';
import { firstDifference, normalizeEvent, normalizeHeaderText, normalizeRun, outcomeRun, settlePhases } from '../src/determinism.mjs';
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
  let hseq = 0;
  return raw.map(([src, ev, members], i) => ({ ...members, ...(src === 'harness' ? { hseq: ++hseq } : {}), ev, seq: i + 1, src, t: t0 + i }));
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
  const h = { ...base, src: 'harness', thread: 'main', hseq: 1 };
  const ok = [
    { ...h, ev: 'client.load_url', url: 'http://10.0.2.2:18090/catalog', extras: ['a'] },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'text/html', encoding: 'utf-8', history_url: 'u', headers: [['a', 'b']], body_len: 1, body_sha256: SHA },
    { ...h, ev: 'client.intercept', null: true },
    { ...h, ev: 'client.intercept', status: 200, reason: 'OK', mime: 'm', encoding: 'e', headers: [], body_len: 0, body_sha256: SHA, complete: false, error: 'IOException' },
    // API 28: the WebResourceResponse of the runtime has status 0 and reason null (android/README.md)
    { ...h, ev: 'client.intercept', status: 0, reason: null, mime: 'm', encoding: 'e', headers: [], body_len: 0, body_sha256: SHA, complete: true, error: null },
    { ...h, ev: 'session.result', src_code: 1000, final_code: 304 },
    { ...h, ev: 'session.message', json: '{}' },
    { ...h, ev: 'session.callback', name: 'onSessionHitCache', args: [] },
    { ...h, ev: 'runtime.cookie_get', url: 'u', value: null },
    { ...h, ev: 'runtime.cookie_set', url: 'u', value: 'a=b' },
    { ...h, ev: 'runtime.cookie_set', url: 'u', value: ['a=b; Path=/', 'c=d'] }, // the list that the SDK passes on
    { ...h, ev: 'webview.intercept_request', url: 'u', method: 'GET' },
    { ...base, src: 'proxy', ev: 'proxy.cut', exchange: 1, bytes: 5 },
    { ...base, src: 'proxy', ev: 'proxy.drop_field', exchange: 1, name: 'etag' },
    { ...base, src: 'proxy', ev: 'proxy.hold', exchange: 1, point: 'body', bytes: 3 },
    { ...base, src: 'proxy', ev: 'proxy.release', exchange: null, point: 'head' },
    { ...base, src: 'conductor', ev: 'step.done', step: { do: 'x' }, error: 'boom' },
    { ...h, ev: 'client.clear_history' },
    { ...h, ev: 'runtime.notify_error', url: 'u', code: 404 },
    { ...h, ev: 'runtime.notify_error', url: 'u', code: 'E_NET' },
    { ...h, ev: 'runtime.toast', text: 'hi', duration: 1 },
    { ...h, ev: 'webview.intercept_request', url: 'u' },
    { ...h, ev: 'webview.page_started', url: 'u' },
    { ...h, ev: 'webview.page_finished', url: 'u' },
    { ...h, ev: 'harness.error', error: 'x' },
    { ...h, ev: 'run.end', ok: false, error: 'abort' },
    { ...h, ev: 'session.result', src_code: 1, final_code: 1, session: 2 },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'a.header', size: 1, sha256: SHA, text: 'ETag: e\n' }, { name: 'a.html', size: 1, sha256: SHA }], db: null },
  ];
  for (const e of ok) assert.deepEqual(eventProblems(e), [], `${e.ev} ${JSON.stringify(e)}`);
  const bad = [
    { ...h, ev: 'client.intercept', null: true, status: 200 },
    { ...h, ev: 'client.intercept', null: false },
    { ...h, ev: 'runtime.cookie_set', url: 'u', value: ['a=b', 1] },
    { ...h, ev: 'webview.intercept_request', url: 'u', method: 1 },
    { ...h, ev: 'client.intercept', status: 0, reason: 0, mime: 'm', encoding: 'e', headers: [], body_len: 0, body_sha256: SHA, complete: true },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'm', encoding: 'e', history_url: 'u', headers: [['a']], body_len: 1, body_sha256: SHA },
    { ...h, ev: 'client.load_data', base_url: 'u', mime: 'm', encoding: 'e', history_url: 'u', headers: [], body_len: 1, body_sha256: 'xyz' },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'b', size: 1, sha256: SHA }, { name: 'a', size: 1, sha256: SHA }], db: null },
    { ...h, ev: 'cache.snapshot', files: [], db: {} },
    { ...base, src: 'proxy', ev: 'proxy.request', exchange: 0, method: 'GET', target: '/', fields: [] },
    { ...base, src: 'conductor', ev: 'run.end', ok: 'yes', error: null },
    { ...h, hseq: undefined, ev: 'client.clear_history' },
    { ...h, hseq: 0, ev: 'client.clear_history' },
    { ...base, src: 'proxy', ev: 'proxy.cut', exchange: 1, bytes: 5, hseq: 1 },
    { ...h, ev: 'client.clear_history', session: 0 },
    { ...base, src: 'proxy', ev: 'proxy.cut', exchange: 1, bytes: 5, session: 1 },
    { ...h, ev: 'runtime.toast', text: 'hi', duration: {} },
    { ...h, ev: 'runtime.notify_error', url: 'u' },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'a.header', size: 1, sha256: SHA }], db: null },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'a.html', size: 1, sha256: SHA, text: 'x' }], db: null },
    { ...h, ev: 'cache.snapshot', files: [{ name: 'a.html', size: 1, sha256: SHA, extra: 1 }], db: null },
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
  assert.ok(problems(run.slice(0, -1)).includes('the last event is not run.end of the conductor'));
  assert.ok(problems(runEvents({ ok: false })).some((p) => p.startsWith('the run did not succeed: x')));
  const back = run.map((e, i) => ({ ...e, t: 100 - i }));
  assert.ok(problems(back).includes('t goes back'));
  assert.ok(problems(run, toText(run).slice(0, -1)).includes('the last line has no LF'));
  assert.deepEqual(checkRunText('').errors, ['empty event log']);
  assert.ok(checkRunText('not json\n').errors.includes('line 1: not JSON'));
  const twice = [...run.slice(0, -1), { ...run[0], seq: run.length }, run.at(-1)].map((e, i) => ({ ...e, seq: i + 1 }));
  assert.ok(problems(twice).includes('more than one run.start or run.end'));
});

test('check_sequence_rejects_hseq_gaps_and_a_harness_abort_and_warns_about_harness_errors', () => {
  const run = runEvents();
  const renumber = (events) => events.map((e, i) => ({ ...e, seq: i + 1 }));
  const gap = run.map((e) => (e.hseq === 3 ? { ...e, hseq: 9 } : e));
  assert.ok(checkRunText(toText(gap)).errors.includes('hseq of the harness events is not 1..n without gaps or repeats'));
  const repeat = run.map((e) => (e.hseq === 3 ? { ...e, hseq: 2 } : e));
  assert.ok(checkRunText(toText(repeat)).errors.includes('hseq of the harness events is not 1..n without gaps or repeats'));
  const late = run.map((e) => (e.hseq === 1 ? { ...e, hseq: 4 } : e.hseq === 4 ? { ...e, hseq: 1 } : e));
  assert.deepEqual(checkRunText(toText(late)).errors, []); // seq order and hseq order may differ (threads)
  const abort = renumber([...run.slice(0, -1), { ev: 'run.end', src: 'harness', thread: 'main', hseq: 5, ok: false, error: 'crash', t: run.at(-1).t }, run.at(-1)]);
  assert.ok(checkRunText(toText(abort)).errors.includes('the harness aborted the run'));
  const warned = renumber([...run.slice(0, -1), { ev: 'harness.error', src: 'harness', thread: 'main', hseq: 5, error: 'oops', t: run.at(-1).t }, run.at(-1)]);
  const res = checkRunText(toText(warned));
  assert.deepEqual(res.errors, []);
  assert.deepEqual(res.warnings, ['harness.error at seq 10: oops']);
  assert.ok(checkRunText(toText(run.slice(0, -1).concat({ ...run.at(-1), src: 'harness', thread: 'main', hseq: 5, ok: false }))).errors.includes('the last event is not run.end of the conductor'));
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
    'CS-01/quick: runs differ (run 3 at line 9) and run.json gives no reason',
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

test('check_determinism_compares_the_recorded_outcome_with_the_projection', async () => {
  const a = toText(runEvents());
  const other = toText(runEvents().map((e) => (e.ev === 'proxy.response_head' ? { ...e, status: 304 } : e)));
  const t = await tree([a, a, other]);
  try {
    const file = path.join(t.root, 'run.json');
    const j = JSON.parse(await (await import('node:fs/promises')).readFile(file, 'utf8'));
    assert.equal(j.summary.unstable[0].outcome, 'unstable');
    assert.deepEqual((await checkTree(t.root)).unstable.map((u) => u.outcome), ['unstable']);
    j.summary.unstable[0].outcome = 'stable';
    await writeFile(file, JSON.stringify(j));
    assert.deepEqual((await checkTree(t.root)).errors, ['CS-01/quick: run.json says outcome stable, the outcome projection is unstable']);
    j.summary.unstable[0].outcome = 'maybe';
    assert.ok(runJsonProblems(j).some((p) => p.includes('outcome?: stable|unstable')));
    delete j.summary.unstable[0].outcome;
    await writeFile(file, JSON.stringify(j));
    assert.deepEqual((await checkTree(t.root)).errors, []);
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

test('determinism_normalizes_t_seq_worker_threads_and_the_android_time_fields', () => {
  const e = {
    ev: 'proxy.response_head', seq: 3, src: 'proxy', t: 99, thread: 'worker-2', hseq: 4,
    fields: [['Date', 'x'], ['ETag', 'e'], ['date', 'y'], ['X-Android-Sent-Millis', '1'], ['x-android-received-millis', '2']],
    extra: { headers: [['DATE', 'z']] },
  };
  assert.deepEqual(normalizeEvent(e), {
    ev: 'proxy.response_head', src: 'proxy',
    fields: [['Date', ''], ['ETag', 'e'], ['date', ''], ['X-Android-Sent-Millis', ''], ['x-android-received-millis', '']],
    extra: { headers: [['DATE', '']] },
  });
  assert.equal(normalizeEvent({ ...e, thread: 'main' }).thread, 'main');
  assert.deepEqual(normalizeEvent({ ev: 'a', args: ['date', 5], other: [['dates', 'keep']] }), { ev: 'a', args: ['date', 5], other: [['dates', 'keep']] });
  assert.deepEqual(normalizeRun([{ ev: 'a', src: 'conductor', t: 1, z: 1, b: 2 }]), ['{"b":2,"ev":"a","src":"conductor","z":1}']);
});

test('determinism_ignores_the_run_number_of_run_start_only', () => {
  // The conductor writes repeat = k of run-<k>.jsonl, so three runs of one mode differ there by construction.
  const runK = (k) => runEvents().map((e) => (e.ev === 'run.start' ? { ...e, repeat: k } : e));
  assert.equal(firstDifference([1, 2, 3].map((k) => normalizeRun(runK(k)))), null);
  assert.equal(normalizeEvent({ ev: 'run.start', repeat: 2, scenario: 'CS-01' }).repeat, undefined);
  assert.equal(normalizeEvent({ ev: 'step.start', repeat: 2 }).repeat, 2); // only run.start
  const otherMode = runK(1).map((e) => (e.ev === 'run.start' ? { ...e, mode: 'standard' } : e));
  assert.notEqual(firstDifference([normalizeRun(runK(1)), normalizeRun(otherMode)]), null);
});

/** A run with a settle step: create_session, a callback of an SDK thread, settle, client_ready. */
function settleRun({ callbackAfterDone = false, callbackAfterSettle = false, mainSwap = false, workerOrder = ['a', 'b'] } = {}) {
  const raw = [
    ['conductor', 'run.start', { scenario: 'CS-01', mode: 'quick', repeat: 1, origin: 'lab-node', upstream_sha: '59936bef' }],
    ['conductor', 'step.start', { step: { do: 'create_session', seq: 1 } }],
    ['harness', 'session.created', { thread: 'main', session_id: 'abc', session: 1 }],
  ];
  const callback = ['harness', 'session.callback', { thread: 'sdk-session', name: 'onSessionFirstLoad', args: [], session: 1 }];
  if (!callbackAfterDone) raw.push(callback);
  raw.push(['harness', 'step.done', { thread: 'instr', step: { do: 'create_session', seq: 1 } }]);
  raw.push(['conductor', 'step.start', { step: { do: 'bind_client', seq: 2 } }]);
  if (callbackAfterDone && !callbackAfterSettle) raw.push(callback);
  raw.push(['harness', 'step.done', { thread: 'instr', step: { do: 'bind_client', seq: 2 } }]);
  raw.push(['conductor', 'step.start', { step: { do: 'settle', ms: 300, seq: 3 } }]);
  for (const n of workerOrder) raw.push(['harness', 'session.callback', { thread: `w-${n}`, name: `on_${n}`, args: [], session: 1 }]);
  raw.push(['conductor', 'step.done', { step: { do: 'settle', ms: 300, seq: 3 } }]);
  raw.push(['conductor', 'step.start', { step: { do: 'client_ready', seq: 4 } }]);
  if (callbackAfterSettle) raw.push(callback);
  const loads = [['harness', 'client.load_url', { thread: 'main', url: 'http://10.0.2.2:18090/catalog', extras: [], session: 1 }],
    ['harness', 'client.clear_history', { thread: 'main', session: 1 }]];
  raw.push(...(mainSwap ? loads.reverse() : loads));
  raw.push(['harness', 'step.done', { thread: 'instr', step: { do: 'client_ready', seq: 4 } }]);
  raw.push(['conductor', 'run.end', { ok: true, error: null }]);
  let hseq = 0;
  return raw.map(([src, ev, members], i) => ({ ...members, ...(src === 'harness' ? { hseq: ++hseq } : {}), ev, seq: i + 1, src, t: i }));
}

test('outcome_projection_ignores_window_and_thread_races_of_sdk_events', () => {
  const base = settleRun();
  const moved = settleRun({ callbackAfterDone: true }); // the SDK callback falls into the next step window
  const swapped = settleRun({ workerOrder: ['b', 'a'] }); // two SDK threads interleave the other way
  assert.notEqual(firstDifference([normalizeRun(base), normalizeRun(moved)]), null); // the strict rule sees a difference
  assert.equal(firstDifference([outcomeRun(base), outcomeRun(moved), outcomeRun(swapped)]), null);
});

test('outcome_projection_keeps_main_thread_order_values_and_settle_phases', () => {
  const base = outcomeRun(settleRun());
  assert.notEqual(firstDifference([base, outcomeRun(settleRun({ mainSwap: true }))]), null); // main-thread order is behavior
  assert.notEqual(firstDifference([base, outcomeRun(settleRun({ callbackAfterDone: true, callbackAfterSettle: true }))]), null); // across a settle
  const changed = settleRun().map((e) => (e.ev === 'session.created' ? { ...e, session_id: 'xyz' } : e));
  assert.notEqual(firstDifference([base, outcomeRun(changed)]), null);
  const phases = settlePhases(settleRun());
  assert.deepEqual(phases.map((p) => p.filter((e) => e.ev === 'step.start').map((e) => e.step.do)), [['create_session', 'bind_client', 'settle'], ['client_ready']]);
  assert.equal(base.filter((l) => l === '--- phase').length, 2);
});

test('determinism_removes_local_refresh_time_from_the_page_message', () => {
  const message = (json) => normalizeEvent({ ev: 'session.message', json }).json;
  assert.equal(message('{"code":200,"local_refresh_time":1234,"srcCode":200}'), '{"code":200,"srcCode":200}');
  assert.equal(message('{"code":200,"srcCode":200}'), '{"code":200,"srcCode":200}');
  assert.equal(message('not json'), 'not json');
  assert.equal(message('[1]'), '[1]');
  assert.equal(normalizeEvent({ ev: 'session.message', json: 5 }).json, 5);
});

test('determinism_compares_header_files_by_their_normalized_text', () => {
  const snap = (text, size, sha256) => normalizeEvent({ ev: 'cache.snapshot', files: [{ name: 'a.html', size: 5, sha256: SHA }, { name: 'a.header', size, sha256, text }], db: null });
  const one = snap('Date: Thu\nETag: e1\nX-Android-Sent-Millis: 1\n', 40, 'b'.repeat(64));
  const two = snap('Date: Fri\nETag: e1\nX-Android-Sent-Millis: 2\n', 41, 'c'.repeat(64));
  assert.deepEqual(one, two);
  assert.deepEqual(one.files[1], { name: 'a.header', text: 'Date:\nETag: e1\nX-Android-Sent-Millis:\n' });
  assert.deepEqual(one.files[0], { name: 'a.html', size: 5, sha256: SHA });
  assert.notDeepEqual(one, snap('Date: Fri\nETag: e2\n', 41, 'c'.repeat(64)));
  assert.equal(normalizeHeaderText('A: b\nno colon\n:x\nx-android-received-millis : 5'), 'A: b\nno colon\n:x\nx-android-received-millis :');
  // the format of the Android harness: lower-case name, space-colon-space, CRLF
  const crlf = 'date : Fri, 09 Oct 2026 10:31:36 GMT\r\netag : e1\r\nX-Android-Sent-Millis : 1\r\nx-android-received-millis : 2\r\ncontent-type : text/html\r\n';
  assert.equal(normalizeHeaderText(crlf), 'date :\r\netag : e1\r\nX-Android-Sent-Millis :\r\nx-android-received-millis :\r\ncontent-type : text/html\r\n');
  assert.equal(normalizeHeaderText('DATE : a\r\n'), 'DATE :\r\n');
  assert.equal(normalizeHeaderText('dated : a\r\nmy-date : b'), 'dated : a\r\nmy-date : b');
  assert.deepEqual(normalizeEvent({ ev: 'cache.snapshot', files: [{ name: 'x.header', size: 1, sha256: SHA }], db: null }).files, [{ name: 'x.header', size: 1, sha256: SHA }]);
});

test('determinism_ignores_the_interleaving_of_sources_inside_a_step_window', () => {
  const ev = (src, ev, extra = {}) => ({ ev, src, t: 0, seq: 0, ...extra });
  const start = ev('conductor', 'step.start', { step: { do: 'request_resource', seq: 1 } });
  const h1 = ev('harness', 'runtime.cookie_get', { thread: 'main', hseq: 1, url: 'u', value: null });
  const h2 = ev('harness', 'client.load_url', { thread: 'main', hseq: 2, url: 'u', extras: [] });
  const p1 = ev('proxy', 'proxy.request', { exchange: 1, method: 'GET', target: '/', fields: [] });
  const p2 = ev('proxy', 'proxy.request', { exchange: 2, method: 'GET', target: '/', fields: [] });
  const a = normalizeRun([start, h1, p1, h2, p2]);
  assert.deepEqual(normalizeRun([start, p1, h2, p2, h1].map((e, i) => ({ ...e, seq: i }))), a); // sources interleave differently, hseq and exchange order are the same
  assert.deepEqual(normalizeRun([start, h2, h1, p1, p2]), a); // the harness emitted h1 first: hseq decides
  assert.notDeepEqual(normalizeRun([start, h1, h2, p2, p1]), normalizeRun([start, h1, h2, p1, p2]).slice().reverse());
  assert.deepEqual(normalizeRun([start, h1, p2, p1, h2]), a); // proxy events by exchange
  // an event in another window is a difference
  const next = ev('conductor', 'step.start', { step: { do: 'destroy', seq: 2 } });
  assert.notDeepEqual(normalizeRun([start, h1, h2, p1, p2, next]), normalizeRun([start, h1, h2, p1, next, p2]));
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
    assert.deepEqual(first.summary.unstable, [{ scenario: 'CS-01', mode: 'quick', reason: 'runs differ: run 2 at line 9', outcome: 'unstable' }]);
    const previous = { emulator_image: 'api28', summary: { unstable: [{ scenario: 'CS-01', mode: 'quick', reason: 'recorded by hand' }] } };
    const second = await buildRunJson({ root: t.root, previous, meta });
    assert.equal(second.summary.unstable[0].reason, 'recorded by hand');
    assert.equal(second.emulator_image, 'api28');
    assert.equal((await buildRunJson({ root: t.root, previous, meta: { ...meta, emulatorImage: 'api29' } })).emulator_image, 'api29');
  } finally {
    await t.done();
  }
});

const IOS_META = { platform: 'ios', macos_version: '26.1.0', sdk_version: '27.0', compiler: 'clang', upstream_sha: '59936bef', cases: 3, limit: 'x', non_public_calls: ['SonicServer -encodingFromHeaders'] };

test('check_functions_jsonl_accepts_unsupported_results_and_extra_members', () => {
  const cases = [{ id: 'FN-1' }, { id: 'FN-2' }, { id: 'FN-3' }];
  const line = (o) => `${JSON.stringify(o)}\n`;
  const text = line({ id: 'FN-1', fn: 'charset', platform: 'ios', result: { value: 'utf-8', ns_string_encoding: 4 } }) +
    line({ id: 'FN-2', fn: 'charset', platform: 'ios', result: { value: 'gbk' }, ns_string_encoding: 2147484425 }) +
    line({ id: 'FN-3', fn: 'message', platform: 'ios', unsupported: 'private session state' });
  assert.deepEqual(functionsProblems(text, 'ios', cases), []);
  assert.ok(functionsProblems(line({ id: 'FN-1', fn: 'message', platform: 'ios', unsupported: '' }), 'ios', cases).some((p) => p.includes('unsupported needs a reason')));
  assert.ok(functionsProblems(line({ id: 'FN-1', fn: 'message', platform: 'ios', unsupported: true }), 'ios', cases).some((p) => p.includes('unsupported needs a reason')));
});

test('check_meta_files_are_validated', () => {
  assert.deepEqual(metaProblems(IOS_META, 'ios'), []);
  assert.deepEqual(metaProblems({}, 'android'), []);
  assert.deepEqual(metaProblems([], 'ios'), ['meta file is not an object']);
  assert.deepEqual(metaProblems({ platform: 'android' }, 'ios'), ['platform must be ios']);
  assert.deepEqual(metaProblems({ non_public_calls: 'x' }, 'ios'), ['non_public_calls must be a list of strings']);
  assert.deepEqual(metaProblems({ non_public_calls: [1] }, 'ios'), ['non_public_calls must be a list of strings']);
  assert.deepEqual(metaProblems({ cases: -1 }, 'ios'), ['cases must be a non-negative integer']);
  assert.deepEqual(metaProblems({ cases: 1.5 }, 'ios'), ['cases must be a non-negative integer']);
  for (const k of ['macos_version', 'sdk_version', 'upstream_sha', 'compiler']) assert.deepEqual(metaProblems({ [k]: 1 }, 'ios'), [`${k} must be a string`]);
});

test('check_run_json_must_carry_the_meta_files_of_the_function_runners', async () => {
  const a = toText(runEvents());
  const t = await tree([a]);
  try {
    await mkdir(path.join(t.root, 'ios'));
    await writeFile(path.join(t.root, 'ios', 'functions.jsonl.meta.json'), JSON.stringify(IOS_META));
    assert.deepEqual((await checkTree(t.root)).errors, ['run.json: functions.ios does not match ios/functions.jsonl.meta.json']);
    const j = await buildRunJson({ root: t.root, meta: { date: '2026-11-05', tools: { node: 'v22' }, upstreamSha: '59936bef', emulatorImage: 'x' } });
    assert.deepEqual(j.functions, { ios: IOS_META });
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify(j));
    assert.deepEqual((await checkTree(t.root)).errors, []);
    await writeFile(path.join(t.root, 'run.json'), JSON.stringify({ ...j, functions: { ios: { ...IOS_META, sdk_version: '1' } } }));
    assert.deepEqual((await checkTree(t.root)).errors, ['run.json: functions.ios does not match ios/functions.jsonl.meta.json']);
    await writeFile(path.join(t.root, 'ios', 'functions.jsonl.meta.json'), '{');
    assert.ok((await checkTree(t.root)).errors.includes('ios/functions.jsonl.meta.json: not valid JSON'));
    await writeFile(path.join(t.root, 'ios', 'functions.jsonl.meta.json'), JSON.stringify({ ...IOS_META, platform: 'android' }));
    assert.ok((await checkTree(t.root)).errors.includes('ios/functions.jsonl.meta.json: platform must be ios'));
  } finally {
    await t.done();
  }
});

test('check_run_json_functions_member_is_validated', () => {
  const base = { date: '2026-11-05', tools: {}, emulator_image: 'x', upstream_sha: '59936bef', scenarios: [], summary: { stable: [], unstable: [] } };
  assert.deepEqual(runJsonProblems({ ...base, functions: { ios: IOS_META, android: {} } }), []);
  assert.ok(runJsonProblems({ ...base, functions: [] }).some((p) => p.includes('functions must map')));
  assert.ok(runJsonProblems({ ...base, functions: { web: {} } }).some((p) => p.includes('functions must map')));
  assert.ok(runJsonProblems({ ...base, functions: { ios: { non_public_calls: 3 } } }).includes('run.json: functions.ios: non_public_calls must be a list of strings'));
});
