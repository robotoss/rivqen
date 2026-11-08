// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The trace projection on small synthetic runs (README "Trace projection and the comparison
// (DL-104)"): the input bounds and the projection errors of a stored run, and the projection
// rules of each member. The stored runs of the lab are in client-trace.test.mjs.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  TRACE_LIMITS, TraceProjectionError, compareClient, labUrl, normalizeClient, parseRun, projectRun, projectRuns, sortedEntries, sortedFields, traceSteps,
} from '../src/client-trace.mjs';

const CTX = { url: 'https://shop.example.com/catalog', account: 'rq-test-account' };
const lines = (...events) => Buffer.from(`${events.map((e) => JSON.stringify(e)).join('\n')}\n`);
const step = (seq, s) => ({ ev: 'step.start', src: 'conductor', seq, step: { ...s, seq } });
const created = (hseq, session, sessionId) => ({ ev: 'session.created', src: 'harness', thread: 'main', hseq, session, session_id: sessionId });
const project = (...events) => projectRun(parseRun(lines(...events)), CTX);

/** The TraceProjectionError of fn (any other exception fails the test). */
function projectionError(fn) {
  try {
    fn();
  } catch (error) {
    assert.ok(error instanceof TraceProjectionError, `not a TraceProjectionError: ${error}`);
    return error;
  }
  assert.fail('no error');
}

// --- parseRun ---

test('parse_run_takes_bytes_only', () => {
  for (const input of [new ArrayBuffer(4), '{"ev":"x"}\n', [123], null]) {
    assert.equal(projectionError(() => parseRun(input)).code, 'TRACE_INPUT', String(input));
  }
  assert.deepEqual(parseRun(new Uint8Array(Buffer.from('{"ev":"x"}\n'))), [{ ev: 'x' }]);
});

test('parse_run_bounds_the_bytes_and_the_events', () => {
  const prefix = '{"ev":"x","p":"';
  const suffix = '"}\n';
  const sized = (n) => Buffer.from(prefix + 'a'.repeat(n - prefix.length - suffix.length) + suffix);
  assert.equal(parseRun(sized(TRACE_LIMITS.bytes)).length, 1);
  assert.equal(projectionError(() => parseRun(sized(TRACE_LIMITS.bytes + 1))).code, 'TRACE_LIMIT');
  const events = (n) => Buffer.from('{"ev":"x"}\n'.repeat(n));
  assert.equal(parseRun(events(TRACE_LIMITS.events)).length, TRACE_LIMITS.events);
  assert.equal(projectionError(() => parseRun(events(TRACE_LIMITS.events + 1))).code, 'TRACE_LIMIT');
});

test('parse_run_errors_name_the_line_and_keep_the_cause', () => {
  const json = projectionError(() => parseRun(Buffer.from('{"ev":"x"}\n\n{"ev":\n')));
  assert.equal(json.code, 'TRACE_JSON');
  assert.match(json.message, /^line 2 /); // empty lines are not counted
  assert.ok(json.cause instanceof SyntaxError);
  const utf8 = projectionError(() => parseRun(Buffer.from([0x7b, 0x80, 0x7d, 0x0a])));
  assert.ok(utf8.cause instanceof TypeError);
  for (const bad of ['null', '{}', '{"ev":1}', '[]', '"ev"']) {
    const error = projectionError(() => parseRun(Buffer.from(`{"ev":"x"}\n{"ev":"y"}\n${bad}\n`)));
    assert.equal(error.code, 'TRACE_EVENT', bad);
    assert.match(error.message, /^line 3 /, bad);
  }
});

// --- helpers of the projection ---

test('lab_url_keeps_a_value_it_cannot_map', () => {
  assert.equal(labUrl(1, CTX.url), 1);
  assert.equal(labUrl(null, CTX.url), null);
  assert.equal(labUrl('http://10.0.2.2:18090/catalog', 'not a url'), 'http://10.0.2.2:18090/catalog');
});

test('sorted_fields_keeps_string_pairs_with_lowercase_names_in_name_order', () => {
  assert.deepEqual(sortedFields(undefined), []);
  assert.deepEqual(sortedFields({ a: 'b' }), []);
  const pairs = [['X-B', '1'], ['a'], ['a', 'b', 'c'], [1, 'x'], ['x', 1], 'ab', ['C', '2'], ['x-b', '0'], ['A', '3']];
  assert.deepEqual(sortedFields(pairs), [['a', '3'], ['c', '2'], ['x-b', '1'], ['x-b', '0']]);
});

test('sorted_entries_sorts_a_copy_by_the_json_text', () => {
  const list = [{ b: 1 }, { a: 2 }, { a: 1 }];
  assert.deepEqual(sortedEntries(list), [{ a: 1 }, { a: 2 }, { b: 1 }]);
  assert.deepEqual(list, [{ b: 1 }, { a: 2 }, { a: 1 }]);
});

test('trace_steps_of_a_step_that_is_not_an_object_are_empty', () => {
  const run = parseRun(lines({ ev: 'step.start', src: 'conductor', step: ['x'] }, { ev: 'step.start', src: 'conductor', step: 'abc' }, { ev: 'step.start', src: 'harness', step: { do: 'x' } }));
  assert.deepEqual(traceSteps(run), [{}, {}]);
});

// --- projectRun ---

test('project_run_takes_an_array_of_events', () => {
  assert.equal(projectionError(() => projectRun({}, CTX)).code, 'TRACE_INPUT');
});

test('requests_are_in_exchange_order_with_their_head_and_end', () => {
  const p = project(
    step(1, { do: 'request_resource' }),
    { ev: 'proxy.request', src: 'proxy', exchange: 2, method: 'GET', target: '/b', fields: [['If-None-Match', 'e'], ['accept-encoding', 'gzip'], ['cookie']] },
    { ev: 'client.load_url', src: 'harness', thread: 'main', hseq: 1, session: 1, url: 'http://10.0.2.2:18090/a' },
    { ev: 'proxy.request', src: 'proxy', exchange: 1, method: 'GET', target: '/a', fields: [] },
    { ev: 'proxy.response_head', src: 'proxy', exchange: 1, status: 200, fields: [['ETag', 'e'], ['content-type', 'text/html'], ['x'], ['set-cookie', 'a=b']] },
    { ev: 'proxy.response_end', src: 'proxy', exchange: 1, complete: true },
    { ev: 'proxy.release', src: 'proxy', exchange: 1 },
    { ev: 'proxy.response_head', src: 'proxy', exchange: 3, status: 200, fields: [] },
    { ev: 'proxy.request', src: 'proxy', exchange: 3, method: 'GET', target: '/c', fields: [] },
  );
  assert.deepEqual(p.phases[0].requests, [
    { exchange: 1, method: 'GET', target: '/a', fields: [], response: { status: 200, fields: [['etag', 'e'], ['set-cookie', 'a=b']], complete: true } },
    { exchange: 2, method: 'GET', target: '/b', fields: [['if-none-match', 'e']], response: null },
    { exchange: 3, method: 'GET', target: '/c', fields: [], response: { status: 200, fields: [], complete: false } },
  ]);
});

test('sessions_follow_the_session_created_order_of_the_harness', () => {
  const p = project(step(1, { do: 'create_session' }), step(2, { do: 'create_session' }), step(3, { do: 'create_session' }), created(2, 2, 'b'), created(1, 1, 'a'), { ev: 'session.created', src: 'harness', hseq: 3 });
  assert.deepEqual(p.sessions, [
    { step: 1, opened: true, session: 1, partition: CTX.account, url: CTX.url },
    { step: 2, opened: true, session: 2, partition: CTX.account, url: CTX.url },
    { step: 3, opened: false, partition: CTX.account, url: CTX.url },
  ]);
  // Only a step.start of the conductor is a create_session step (another event with a step object is not).
  const end = { ev: 'step.end', src: 'conductor', step: { do: 'create_session' } };
  assert.equal(project(step(1, { do: 'create_session' }), end, created(1, 1, 'a')).sessions.length, 1);
});

test('cookie_events_become_sorted_get_and_set_entries', () => {
  const cookie = (ev, value, url = 'http://10.0.2.2:18090/catalog') => ({ ev, src: 'harness', thread: 'bg', url, ...(value === undefined ? {} : { value }) });
  const p = project(step(1, { do: 'request_resource' }), cookie('runtime.cookie_set', 'a=1'), cookie('runtime.cookie_set', ['b=2', 'c=3']), cookie('runtime.cookie_set', null), cookie('runtime.cookie_set'), cookie('runtime.cookie_get', 'a=1'), cookie('runtime.cookie_get'));
  assert.deepEqual(p.phases[0].cookies, [
    { call: 'get', url: CTX.url, value: 'a=1' },
    { call: 'get', url: CTX.url, value: null },
    { call: 'set', url: CTX.url, values: ['a=1'] },
    { call: 'set', url: CTX.url, values: ['b=2', 'c=3'] },
    { call: 'set', url: CTX.url, values: [] },
    { call: 'set', url: CTX.url, values: [] },
  ]);
});

test('actions_are_the_main_thread_harness_events_in_hseq_order', () => {
  const p = project(
    step(1, { do: 'client_ready' }),
    { ev: 'client.load_data', src: 'harness', thread: 'main', hseq: 2, session: 1, base_url: 'http://10.0.2.2:18090/catalog', headers: [['B', '2'], ['a', '1']], body_len: 3, body_sha256: 'h' },
    { ev: 'client.load_url', src: 'harness', thread: 'main', hseq: 1, session: 1, url: 'http://10.0.2.2:18090/catalog' },
    { ev: 'client.load_url', src: 'harness', thread: 'bg', hseq: 0, session: 1, url: 'http://10.0.2.2:18090/x' },
    { ev: 'client.load_url', src: 'conductor', thread: 'main', hseq: 0, session: 1, url: 'http://10.0.2.2:18090/y' },
  );
  assert.deepEqual(p.phases[0].actions, [
    { do: 'load_url', session: 1, url: CTX.url },
    { do: 'load_data', session: 1, url: CTX.url, fields: [['a', '1'], ['b', '2']], body_len: 3, body_sha256: 'h' },
  ]);
});

test('a_run_without_steps_has_one_phase_without_steps', () => {
  const p = project({ ev: 'run.start', src: 'conductor' });
  assert.deepEqual(p.phases, [{ steps: [], requests: [], cookies: [], actions: [], intercepts: [], results: [], snapshots: [] }]);
});

test('a_phase_without_a_conductor_step_joins_the_previous_phase', () => {
  const intercept = (session) => ({ ev: 'client.intercept', src: 'harness', thread: 'io', session, null: true });
  const result = (session) => ({ ev: 'session.result', src: 'harness', session, src_code: 1000, final_code: 1000 });
  const p = project(
    step(1, { do: 'request_resource' }),
    intercept(2),
    result(2),
    { ev: 'proxy.request', src: 'proxy', exchange: 1, method: 'GET', target: '/a', fields: [] },
    step(2, { do: 'settle', ms: 10 }),
    { ev: 'step.start', src: 'harness', step: { do: 'x' } },
    intercept(1),
    result(1),
    { ev: 'proxy.request', src: 'proxy', exchange: 2, method: 'GET', target: '/b', fields: [] },
    { ev: 'client.load_url', src: 'harness', thread: 'main', hseq: 1, session: 1, url: CTX.url },
  );
  assert.equal(p.phases.length, 1);
  assert.deepEqual(p.phases[0].steps, [1, 2]);
  assert.deepEqual(p.phases[0].requests.map((r) => r.exchange), [1, 2]);
  assert.deepEqual(p.phases[0].intercepts, [{ session: 1, null: true }, { session: 2, null: true }]);
  assert.deepEqual(p.phases[0].results.map((r) => r.session), [1, 2]);
  assert.equal(p.phases[0].actions.length, 1);
});

// --- cache snapshots ---

const snapshotRun = (files, ...more) => [step(1, { do: 'create_session' }), created(1, 1, 'sid'), ...more, step(2, { do: 'snapshot_cache' }), { ev: 'cache.snapshot', src: 'harness', files }];

test('a_snapshot_record_holds_the_hashes_and_the_header_fields', () => {
  const header = { name: 'sid.header', text: 'ETag : "e"\r\ntemplate-tag: t\n\nX-B : 2\nContent-Type : text/html\n' };
  const p = project(...snapshotRun([header, { name: 'sid.html', sha256: 'p' }, { name: 'sid.tpl', sha256: 1 }, { name: 'sid.html.tmp' }]));
  assert.deepEqual(p.phases[0].snapshots, [{
    entries: [{ partition: CTX.account, url: CTX.url, vary: [], etag: '"e"', template_tag: 't', page_sha256: 'p', template_sha256: null, data_sha256: null, fields: [['content-type', 'text/html'], ['etag', '"e"'], ['template-tag', 't'], ['x-b', '2']] }],
  }]);
});

test('snapshot_records_are_sorted_by_partition', () => {
  const p = project(
    step(1, { do: 'set_account', value: 'z' }), step(2, { do: 'create_session' }), created(1, 1, 'z1'),
    step(3, { do: 'set_account', value: null }), step(4, { do: 'create_session' }), created(2, 2, 'n1'),
    step(5, { do: 'set_account', value: 'a' }), step(6, { do: 'create_session' }), created(3, 3, 'a1'),
    step(7, { do: 'snapshot_cache' }), { ev: 'cache.snapshot', src: 'harness', files: [{ name: 'z1.html' }, { name: 'a1.html' }, { name: 'n1.html' }] },
  );
  assert.deepEqual(p.phases[0].snapshots[0].entries.map((r) => r.partition), ['a', 'z', null]); // the JSON text: a string sorts before null
});

test('a_bad_snapshot_is_a_projection_error', () => {
  const bad = [
    [undefined, 'TRACE_SNAPSHOT'],
    [[null], 'TRACE_SNAPSHOT'],
    [[{ name: 1 }], 'TRACE_SNAPSHOT'],
    [[{ name: 'sid.db' }], 'TRACE_SNAPSHOT'],
    [[{ name: 'sid' }], 'TRACE_SNAPSHOT'],
    [[{ name: 'other.html.tmp' }], 'TRACE_SNAPSHOT'],
    [[{ name: 'sid.header' }], 'TRACE_SNAPSHOT'],
    [[{ name: 'sid.header', text: 'etag "e"' }], 'TRACE_HEADER'],
  ];
  for (const [files, code] of bad) assert.equal(projectionError(() => project(...snapshotRun(files))).code, code, JSON.stringify(files));
});

// --- normalization and the comparison ---

const client = (phase = {}) => ({ sessions: [{ step: 1, opened: true, session: 1, partition: null, url: CTX.url }], phases: [{ steps: [1, 1], requests: [], cookies: [], actions: [], intercepts: [], results: [], snapshots: [], ...phase }] });

test('normalization_changes_only_the_compared_members', () => {
  const n = normalizeClient(client({
    actions: [{ do: 'load_url', session: 1, url: CTX.url }, { do: 'load_data', session: 1, url: CTX.url, fields: [['etag', 'ABC'], ['date', 'x']], body_len: 1, body_sha256: 'h' }],
    intercepts: [{ session: 2, null: true }, { session: 1, status: 0, fields: [['Content-Type', 'X']], body_len: 1, body_sha256: 'h', complete: true }],
    snapshots: [{ entries: [{ partition: null, url: CTX.url, vary: [], etag: null, template_tag: null, page_sha256: null, template_sha256: null, data_sha256: null, fields: [] }] }],
  }));
  const [phase] = n.phases;
  assert.deepEqual(phase.actions, [{ do: 'load_url', session: 1, url: CTX.url }, { do: 'load_data', session: 1, url: CTX.url, fields: [['etag', 'ABC']], body_len: 1, body_sha256: 'h' }]);
  assert.deepEqual(phase.intercepts, [{ session: 1, status: 0, fields: [['Content-Type', 'X']], body_len: 1, body_sha256: 'h', complete: true }, { session: 2, null: true }]);
  assert.equal(phase.snapshots[0].entries[0].data_sha256, false);
});

test('compare_client_names_the_first_different_item', () => {
  const cookie = { call: 'get', url: CTX.url, value: null };
  assert.equal(compareClient(client({ cookies: [cookie, cookie] }), client({ cookies: [cookie] })), `first difference at item 4: expected phase 1 cookies ${JSON.stringify(cookie)}; trace (nothing)`);
  assert.equal(compareClient(client(), client({ cookies: [cookie] })), `first difference at item 3: expected (nothing); trace phase 1 cookies ${JSON.stringify(cookie)}`);
  const other = client();
  other.sessions[0].opened = false;
  assert.match(compareClient(client(), other), /^first difference at item 1: expected session \{/);
  assert.equal(compareClient(client(), client({ steps: [1, 2] })), 'first difference at item 2: expected phase 1 steps [1,1]; trace phase 1 steps [1,2]');
  // A member outside the compared items still makes the members different.
  const extra = client();
  extra.phases[0].extra = 1;
  assert.equal(compareClient(client(), extra), 'the projections differ');
});

test('dl120_u13_takes_requests_without_a_response_and_trace_requests_that_the_expected_member_lacks', () => {
  const u13 = { notReproduced: 'U-13' };
  const request = (exchange, response) => ({ exchange, method: 'GET', target: '/catalog', fields: [], response });
  const held = request(1, null);
  const error = request(2, { status: 500, fields: [['etag', 'e']], complete: true });
  // A request without a response (held, server down) has no U-13 fields; it is compared as usual.
  // A trace request that the expected member does not have can still show U-13.
  assert.equal(compareClient(client({ requests: [held] }), client({ requests: [held, error] }), u13), 'first difference at item 4: expected (nothing); trace phase 1 requests {"exchange":2,"method":"GET","target":"/catalog","fields":[],"response":{"status":500,"fields":[],"complete":true}}');
  assert.match(compareClient(client({ requests: [held] }), client({ requests: [held] }), u13), /^U-13: the trace does not show/);
});

test('project_runs_names_the_run_and_the_item', () => {
  const ok = [step(1, { do: 'request_resource' })];
  const more = [...ok, { ev: 'runtime.cookie_get', src: 'harness', url: CTX.url }];
  assert.deepEqual(projectRuns([], CTX), { error: 'no stored run' });
  assert.equal(projectRuns([ok, ok, 'x'], CTX).error, 'run 3: events must be an array');
  assert.equal(projectRuns([ok, more], CTX).error, 'run 2 differs from run 1 at item 2: (nothing) / phase 1 cookies {"call":"get","url":"https://shop.example.com/catalog","value":null}');
  assert.match(projectRuns([more, ok], CTX).error, /^run 2 differs from run 1 at item 2: phase 1 cookies \{.*\} \/ \(nothing\)$/);
  assert.deepEqual(projectRuns([ok], CTX).client.phases[0].steps, [1, 1]);
  // Only a projection error is a problem text; another exception is a defect and propagates.
  assert.throws(() => projectRuns([ok], null), TypeError);
});
