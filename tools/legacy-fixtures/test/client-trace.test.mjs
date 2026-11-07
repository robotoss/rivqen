// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The trace projection (README "Trace projection and the comparison (DL-104)") and the
// client case checker.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientCaseError, validateBehavior, validateClientCase } from '../src/client-case.mjs';
import { TraceProjectionError, compareClient, labUrl, normalizeClient, parseRun, projectRun, projectRuns, traceSteps } from '../src/client-trace.mjs';
import { scenarioCase, storedRuns } from './client-helpers.mjs';

const CTX = { url: 'https://shop.example.com/catalog', account: 'rq-test-account' };
const lines = (...events) => Buffer.from(`${events.map((e) => JSON.stringify(e)).join('\n')}\n`);
const step = (seq, s) => ({ ev: 'step.start', src: 'conductor', seq, step: { ...s, seq } });

test('a_stored_run_becomes_the_projection_with_phases_at_settle', () => {
  const p = projectRun(storedRuns('CS-04', 'quick')[0], CTX);
  assert.deepEqual(p.sessions.map((s) => s.step), [1, 9]);
  assert.deepEqual(p.phases.map((x) => x.steps), [[1, 6], [7, 15], [16, 18], [19, 21]]);
  assert.equal(p.phases[1].requests[0].response.fields[3][0], 'template-change');
  assert.equal(p.phases[0].cookies[0].url, 'https://shop.example.com/catalog');
});

test('all_runs_of_a_stored_mode_give_one_projection', () => {
  for (const [s, m] of [['CS-01', 'quick'], ['CS-13a', 'standard'], ['CS-16', 'quick']]) assert.equal(projectRuns(storedRuns(s, m), CTX).error, undefined);
});

test('runs_that_differ_are_reported_with_the_item', () => {
  const [a] = storedRuns('CS-01', 'quick');
  const b = a.map((e) => (e.ev === 'session.result' ? { ...e, final_code: 1000 } : e));
  assert.match(projectRuns([a, b], CTX).error, /run 2 differs from run 1/);
});

test('a_cache_file_stem_that_is_no_session_is_a_projection_error', () => {
  const run = parseRun(lines(step(1, { do: 'snapshot_cache' }), { ev: 'cache.snapshot', src: 'harness', files: [{ name: 'other.html', sha256: 'x' }] }));
  assert.throws(() => projectRun(run, CTX), TraceProjectionError);
});

test('an_unknown_cache_file_is_a_projection_error', () => {
  const run = parseRun(lines(step(1, { do: 'snapshot_cache' }), { ev: 'cache.snapshot', src: 'harness', files: [{ name: 'x.db' }] }));
  assert.throws(() => projectRun(run, CTX), { code: 'TRACE_SNAPSHOT' });
});

test('a_line_that_is_not_json_is_a_projection_error', () => {
  assert.throws(() => parseRun(Buffer.from('{"ev":"x"}\nnot json\n')), { code: 'TRACE_JSON' });
  assert.throws(() => parseRun(Buffer.from('[1]\n')), { code: 'TRACE_EVENT' });
});

test('a_create_session_without_session_created_is_a_projection_error', () => {
  assert.throws(() => projectRun(parseRun(lines(step(1, { do: 'create_session' }))), CTX), { code: 'TRACE_SESSION' });
});

test('more_session_created_events_than_create_session_steps_is_a_projection_error', () => {
  const created = (hseq) => ({ ev: 'session.created', src: 'harness', hseq, session: hseq, session_id: `id${hseq}` });
  const run = parseRun(lines(step(1, { do: 'create_session' }), created(1), created(2)));
  assert.throws(() => projectRun(run, CTX), { code: 'TRACE_SESSION', message: /2 session\.created events for 1 create_session steps/ });
  assert.equal(projectRun(parseRun(lines(step(1, { do: 'create_session' }), created(1))), CTX).sessions.length, 1);
});

test('a_run_that_is_not_valid_utf8_is_a_trace_input_error', () => {
  const good = lines({ ev: 'x', v: 'é' });
  assert.equal(parseRun(good)[0].v, 'é');
  // A lone continuation byte inside a string value: a lenient decoder would give U+FFFD and pass.
  const bad = Buffer.concat([Buffer.from('{"ev":"x","v":"'), Buffer.from([0x80]), Buffer.from('"}\n')]);
  assert.throws(() => parseRun(bad), { code: 'TRACE_INPUT', message: /not valid UTF-8/ });
  // A byte order mark is kept: the first line is then not JSON.
  assert.throws(() => parseRun(Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), good])), { code: 'TRACE_JSON' });
});

test('a_refused_session_has_opened_false_and_set_account_changes_the_partition', () => {
  const run = parseRun(lines(step(1, { do: 'set_account', value: 'rq-test-account-2' }), step(2, { do: 'create_session' }), { ev: 'session.created', src: 'harness', hseq: 1, session_id: null }));
  assert.deepEqual(projectRun(run, CTX).sessions, [{ step: 2, opened: false, partition: 'rq-test-account-2', url: CTX.url }]);
});

test('lab_urls_become_the_fixture_url', () => {
  assert.equal(labUrl('http://10.0.2.2:18090/catalog?x=1', 'https://a.example.com/p'), 'https://a.example.com/catalog?x=1');
  assert.equal(labUrl('https://other/', CTX.url), 'https://other/');
});

test('normalization_removes_transport_fields_and_compares_data_by_presence', () => {
  const record = { partition: 'p', url: 'u', vary: [], etag: 'e', template_tag: 't', page_sha256: 'a', template_sha256: 'b', data_sha256: 'c', fields: [['content-encoding', 'gzip'], ['x-android-sent-millis', '1'], ['content-type', 'x']] };
  const client = { sessions: [], phases: [{ steps: [1, 1], requests: [], cookies: [], actions: [], intercepts: [{ session: 1, status: 0, fields: [], body_len: 3, body_sha256: 'h', complete: false }], results: [], snapshots: [{ entries: [record] }] }] };
  const n = normalizeClient(client);
  assert.deepEqual(n.phases[0].snapshots[0].entries[0].fields, [['content-type', 'x']]);
  assert.equal(n.phases[0].snapshots[0].entries[0].data_sha256, true);
  assert.deepEqual(n.phases[0].intercepts[0], { session: 1, status: 0, fields: [], complete: false });
  const other = structuredClone(client);
  other.phases[0].snapshots[0].entries[0].data_sha256 = 'different';
  other.phases[0].intercepts[0].body_len = 9;
  assert.equal(compareClient(client, other), null);
  other.phases[0].snapshots[0].entries[0].etag = 'f';
  assert.match(compareClient(client, other), /phase 1 snapshots/);
});

// A client member with one load_data, one intercept, one record and one request.
function member({ type = 'text/html; charset=utf-8', status = 500, responseFields = [] } = {}) {
  const fields = [['content-type', type]];
  return {
    sessions: [],
    phases: [{
      steps: [1, 1],
      requests: [{ exchange: 1, method: 'GET', target: '/catalog', fields: [], response: { status, fields: responseFields, complete: true } }],
      cookies: [],
      actions: [{ do: 'load_data', session: 1, url: CTX.url, fields, body_len: 0, body_sha256: 'x' }],
      intercepts: [{ session: 1, status: 0, fields, body_len: 0, body_sha256: 'x', complete: true }],
      results: [],
      snapshots: [{ entries: [{ partition: null, url: CTX.url, vary: [], etag: null, template_tag: null, page_sha256: null, template_sha256: null, data_sha256: null, fields }] }],
    }],
  };
}

test('dl119_content_type_compares_as_a_media_type_in_load_data_intercepts_and_records', () => {
  for (const same of ['text/html;charset=utf-8', 'TEXT/HTML; Charset=UTF-8', 'text/html ;charset="utf-8"']) {
    assert.equal(compareClient(member(), member({ type: same })), null, same);
  }
  for (const other of ['text/html; charset=gbk', 'text/plain; charset=utf-8', 'text/html', 'text/html; charset=utf-8; q=1']) {
    assert.match(compareClient(member(), member({ type: other })), /first difference at item \d+: expected phase 1 actions/, other);
  }
  // Only the actions differ: the intercept and the record fields are compared the same way.
  const onlyRecord = member();
  onlyRecord.phases[0].snapshots[0].entries[0].fields = [['content-type', 'text/html; charset=gbk']];
  assert.match(compareClient(member(), onlyRecord), /phase 1 snapshots/);
});

test('dl120_u13_skips_the_legacy_fields_of_a_response_outside_the_scope_and_needs_one_in_the_trace', () => {
  const legacy = [['etag', 'e'], ['template-tag', 't'], ['cache-offline', 'true'], ['template-change', 'false'], ['sonic-etag-key', 'etag']];
  const u13 = { notReproduced: 'U-13' };
  assert.match(compareClient(member(), member({ responseFields: legacy })), /phase 1 requests/);
  assert.equal(compareClient(member(), member({ responseFields: legacy }), u13), null);
  for (const [name, value] of legacy) assert.equal(compareClient(member(), member({ responseFields: [[name, value]] }), u13), null, name);
  // Both sides lose the U-13 fields: other values on the expected side do not count.
  assert.equal(compareClient(member({ responseFields: [['etag', 'x']] }), member({ responseFields: [['etag', 'y'], ['template-tag', 't']] }), u13), null);
  // set-cookie is not a U-13 field: it is still compared.
  assert.match(compareClient(member(), member({ responseFields: [...legacy, ['set-cookie', 'a=b']] }), u13), /phase 1 requests/);
  // The trace must show the difference.
  assert.match(compareClient(member(), member(), u13), /^U-13: the trace does not show the documented difference/);
  assert.match(compareClient(member({ responseFields: legacy }), member({ responseFields: legacy }), u13), /^U-13: the trace does not show/);
  // A 200 or a 304 is in the scope: its fields are compared, and it does not show U-13.
  for (const status of [200, 304]) {
    assert.match(compareClient(member({ status }), member({ status, responseFields: legacy }), u13), /^U-13: the trace does not show/, String(status));
  }
  assert.equal(compareClient(member(), member(), { notReproduced: 'U-11' }), 'not_reproduced "U-11" has no client check');
});

test('trace_steps_drop_seq_and_equal_the_scenario_case_steps', () => {
  const steps = traceSteps(storedRuns('CS-13a', 'quick')[0]);
  assert.deepEqual(steps[0], { bytes: 128, do: 'cut' });
  assert.deepEqual(scenarioCase('CS-13a', 'quick').schedule.filter((s) => s.do).length, steps.length);
});

// --- the case checker ---

const valid = () => structuredClone(scenarioCase('CS-04', 'quick'));
const code = (fn) => {
  try {
    fn();
  } catch (error) {
    assert.ok(error instanceof ClientCaseError);
    return error.code;
  }
  return null;
};

test('a_valid_case_passes_and_gets_the_config_defaults', () => {
  const c = validateClientCase('FX-LEG-CL-CS04-Q', valid());
  assert.equal(c.kind, 'client-canonical');
  assert.equal(c.config.unavailable_time_ms, 21_600_000);
  assert.equal(c.config.authenticated_cache, false); // CL-03, DL-123
  assert.equal(c.url, 'https://shop.example.com/catalog');
});

test('case_errors_name_the_member', () => {
  const cases = [
    [(c) => { c.extra = 1; }, 'CASE_UNKNOWN_MEMBER'],
    [(c) => { c.config.nope = true; }, 'CASE_UNKNOWN_MEMBER'],
    [(c) => { c.config.accept_diff = 'yes'; }, 'CASE_TYPE'],
    [(c) => { c.config.authenticated_cache = 'true'; }, 'CASE_TYPE'],
    [(c) => { c.schedule.push({ do: 'jump' }); }, 'CASE_STEP'],
    [(c) => { c.schedule.push({ do: 'settle', ms: 3_600_001 }); }, 'CASE_LIMIT'],
    [(c) => { c.schedule.push({ do: 'hold', point: 'head', bytes: 1 }); }, 'CASE_STEP'],
    [(c) => { c.schedule.push({ do: 'wait', for: 'webview.page_started' }); }, 'CASE_STEP'],
    [(c) => { c.schedule.push({ case: 'missing' }); }, 'CASE_TYPE'],
    [(c) => { c.url = 'https://evil.test/x'; }, 'CASE_URL'],
    [(c) => { c.cache_before = [{ page: 'a' }]; }, 'CASE_TYPE'],
    [(c) => { c.mode = 'standard'; }, 'CASE_TYPE'],
    [(c) => { c.rows = ['Q23']; }, 'CASE_TYPE'],
    [(c) => { c.rules = ['C-01']; }, 'CASE_TYPE'],
    [(c) => { delete c.schedule; }, 'CASE_MISSING_MEMBER'],
    [(c) => { c.schedule = Array.from({ length: 257 }, () => ({ do: 'release' })); }, 'CASE_LIMIT'],
    [(c) => { c.responses = Object.fromEntries(Array.from({ length: 33 }, (_, i) => [`r${i}`, { page: 'a' }])); }, 'CASE_LIMIT'],
  ];
  for (const [change, want] of cases) {
    const c = valid();
    change(c);
    assert.equal(code(() => validateClientCase('FX-LEG-CL-CS04-Q', c)), want, String(change));
  }
});

test('a_profile_case_has_only_its_own_members', () => {
  const p = { id: 'FX-LEG-CL-CS04-Q--ANDROID', kind: 'client', description: 'x', profile: 'legacy-android', replaces: 'FX-LEG-CL-CS04-Q', divergences: ['D-25'], sources: ['docs:x'] };
  assert.equal(validateClientCase(p.id, p).kind, 'client-profile');
  assert.equal(code(() => validateClientCase(p.id, { ...p, schedule: [] })), 'CASE_UNKNOWN_MEMBER');
  assert.equal(code(() => validateClientCase(p.id, { ...p, replaces: 'FX-LEG-CL-CS05-Q' })), 'CASE_TYPE');
  assert.equal(code(() => validateClientCase('FX-LEG-CL-X', p)), 'CASE_NAME');
});

test('validate_behavior_rejects_any_shape_with_a_case_error', () => {
  for (const v of [null, 1, 'x', [], {}, { mode: 'quick' }, { mode: 'quick', server_profile: 'legacy-node', responses: {}, cache_before: [], schedule: [] }]) {
    assert.ok(code(() => validateBehavior(v)) !== null);
  }
});
