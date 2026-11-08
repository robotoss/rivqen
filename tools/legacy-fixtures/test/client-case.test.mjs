// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The check of a client case.json (src/client-case.mjs; fixtures/legacy/README.md "Client
// fixtures"). A case error is a problem of check 6 "with the member name" (README), so each
// rejection asserts its code and the member path in the message, and nothing else of the text.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientCaseError, LIMITS, STEP_MEMBERS, validateBehavior, validateClientCase } from '../src/client-case.mjs';
import { UPSTREAM } from './client-helpers.mjs';

const NAME = 'FX-LEG-CL-T-Q';

/** A small valid canonical case (code-reading evidence, so that cache_before can have entries). */
const canonical = () => ({
  id: NAME,
  kind: 'client',
  description: 'one behavior',
  profile: 'legacy-canonical',
  mode: 'quick',
  server_profile: 'legacy-canonical',
  rules: ['CL-01'],
  rows: [],
  upstream: { ...UPSTREAM },
  sources: ['docs:engineering/protocol/legacy-client#_9'],
  evidence: 'code-reading',
  responses: { a: { page: 'a' } },
  cache_before: [],
  schedule: [{ case: 'a' }, { do: 'create_session' }],
});

/** The behavior members only. */
const behavior = (change = () => {}) => {
  const c = { mode: 'quick', server_profile: 'legacy-canonical', responses: { a: { page: 'a' } }, cache_before: [], schedule: [{ case: 'a' }, { do: 'create_session' }] };
  change(c);
  return c;
};

/** The ClientCaseError of fn, or null when fn returns. Any other exception fails the test. */
function caseError(fn) {
  try {
    fn();
  } catch (error) {
    assert.ok(error instanceof ClientCaseError, `not a ClientCaseError: ${error}`);
    return error;
  }
  return null;
}

/** fn throws a case error with this code, and the message names this member. */
function rejects(fn, code, member, label = member) {
  const error = caseError(fn);
  assert.ok(error !== null, `${label}: accepted`);
  assert.equal(error.code, code, `${label}: ${error.message}`);
  assert.ok(error.message.includes(member), `${label}: the message does not name ${member}: ${error.message}`);
}

const accepts = (fn, label) => assert.equal(caseError(fn), null, label);

/** A behavior case whose schedule is [case a, step]. */
const withStep = (step) => behavior((c) => { c.schedule = [{ case: 'a' }, step]; });
const stepOf = (step) => validateBehavior(withStep(step)).schedule[1];

// --- config ---

test('config_defaults_are_the_readme_table', () => {
  assert.deepEqual(validateBehavior(behavior()).config, {
    account: 'rq-test-account',
    cookie: null,
    network_valid: true,
    accept_diff: true,
    account_related: true,
    reload_in_bad_network: false,
    support_cache_control: false,
    support_local_server: false,
    auto_start: true,
    get_cookie_when_session_create: true,
    cache_verify_with_sha1: true,
    authenticated_cache: false,
    unavailable_time_ms: 21_600_000,
    connect_timeout_ms: 5000,
    read_timeout_ms: 15000,
  });
});

test('config_account_and_cookie_take_a_string_or_null_only', () => {
  for (const key of ['account', 'cookie']) {
    for (const value of [null, '', 'x', 'x'.repeat(LIMITS.maxText)]) {
      assert.equal(validateBehavior(behavior((c) => { c.config = { [key]: value }; })).config[key], value, `${key} ${String(value).length}`);
    }
    for (const value of [1, true, ['x'], 'x'.repeat(LIMITS.maxText + 1)]) {
      rejects(() => validateBehavior(behavior((c) => { c.config = { [key]: value }; })), 'CASE_TYPE', `config.${key}`);
    }
  }
});

test('config_numbers_outside_their_range_are_rejected', () => {
  const bad = [['unavailable_time_ms', 86_400_001], ['unavailable_time_ms', -1], ['connect_timeout_ms', 3_600_001], ['read_timeout_ms', 1.5]];
  for (const [key, value] of bad) rejects(() => validateBehavior(behavior((c) => { c.config = { [key]: value }; })), 'CASE_TYPE', `config.${key}`, `${key}=${value}`);
  assert.equal(validateBehavior(behavior((c) => { c.config = { unavailable_time_ms: 86_400_000 }; })).config.unavailable_time_ms, 86_400_000);
});

test('config_must_be_an_object', () => {
  rejects(() => validateBehavior(behavior((c) => { c.config = []; })), 'CASE_TYPE', 'config');
});

// --- top-level behavior members ---

test('mode_and_server_profile_must_be_known_words', () => {
  rejects(() => validateBehavior(behavior((c) => { c.mode = 'slow'; })), 'CASE_TYPE', 'mode');
  rejects(() => validateBehavior(behavior((c) => { c.server_profile = 'legacy-android'; })), 'CASE_TYPE', 'server_profile');
  for (const profile of ['legacy-canonical', 'legacy-node', 'legacy-java', 'legacy-php']) {
    assert.equal(validateBehavior(behavior((c) => { c.server_profile = profile; })).server_profile, profile);
  }
});

test('responses_need_1_to_32_entries_with_a_valid_name', () => {
  rejects(() => validateBehavior(behavior((c) => { c.responses = {}; })), 'CASE_LIMIT', 'responses');
  rejects(() => validateBehavior(behavior((c) => { c.responses = []; })), 'CASE_TYPE', 'responses');
  const many = Object.fromEntries(Array.from({ length: LIMITS.responses }, (_, i) => [`r${i}`, { page: 'a' }]));
  assert.equal(validateBehavior(behavior((c) => { c.responses = { ...many, a: { page: 'a' } }; delete c.responses.r0; })).responses.size, LIMITS.responses);
  for (const name of ['Ab', 'aB', 'a_b', 'a'.repeat(33)]) {
    rejects(() => validateBehavior(behavior((c) => { c.responses[name] = { page: 'a' }; })), 'CASE_TYPE', `responses.${name}`);
  }
  assert.ok(validateBehavior(behavior((c) => { c.responses['a-0'.padEnd(32, '9')] = { page: 'a' }; })).responses.has('a-0'.padEnd(32, '9')));
});

test('cache_before_and_schedule_have_their_limits', () => {
  const entry = { page: 'a' };
  assert.equal(validateBehavior(behavior((c) => { c.cache_before = Array.from({ length: LIMITS.cacheBefore }, () => entry); })).cache_before.length, LIMITS.cacheBefore);
  rejects(() => validateBehavior(behavior((c) => { c.cache_before = Array.from({ length: LIMITS.cacheBefore + 1 }, () => entry); })), 'CASE_LIMIT', 'cache_before');
  rejects(() => validateBehavior(behavior((c) => { c.cache_before = {}; })), 'CASE_LIMIT', 'cache_before');
  rejects(() => validateBehavior(behavior((c) => { c.schedule = {}; })), 'CASE_LIMIT', 'schedule');
});

test('the_step_limit_counts_steps_not_case_entries', () => {
  // 256 steps between case entries pass; a case entry is not a step.
  const steps = Array.from({ length: LIMITS.steps }, () => ({ do: 'release' }));
  assert.equal(validateBehavior(behavior((c) => { c.schedule = [{ case: 'a' }, ...steps, { case: 'a' }]; })).schedule.length, LIMITS.steps + 2);
  rejects(() => validateBehavior(behavior((c) => { c.schedule = [{ case: 'a' }, ...steps, { do: 'release' }]; })), 'CASE_LIMIT', 'schedule');
  // Only case entries: no step.
  rejects(() => validateBehavior(behavior((c) => { c.schedule = [{ case: 'a' }]; })), 'CASE_LIMIT', 'schedule');
  // The list itself is bounded (4 x 256 items) before any item is read.
  const cases = Array.from({ length: 4 * LIMITS.steps }, () => ({ case: 'a' }));
  assert.equal(validateBehavior(behavior((c) => { c.schedule = [...cases.slice(1), { do: 'release' }]; })).schedule.length, 4 * LIMITS.steps);
  rejects(() => validateBehavior(behavior((c) => { c.schedule = [...cases, { do: 'release' }]; })), 'CASE_LIMIT', 'schedule');
});

test('at_most_16_create_session_steps', () => {
  const sessions = (n) => Array.from({ length: n }, () => ({ do: 'create_session' }));
  assert.equal(validateBehavior(behavior((c) => { c.schedule = [{ case: 'a' }, ...sessions(LIMITS.sessions), { do: 'release' }]; })).schedule.length, LIMITS.sessions + 2);
  rejects(() => validateBehavior(behavior((c) => { c.schedule = [{ case: 'a' }, ...sessions(LIMITS.sessions + 1)]; })), 'CASE_LIMIT', 'create_session');
});

// --- url ---

test('the_page_url_must_be_http_or_https_on_example_com', () => {
  for (const url of ['https://example.com/p', 'http://example.com/p', 'https://a.b.example.com/p?q=1']) {
    assert.equal(validateBehavior(behavior((c) => { c.url = url; })).url, url);
  }
  for (const url of ['ftp://example.com/p', 'https://badexample.com/p', 'https://example.com.evil.test/p', 'https://u@example.com/p', 'https://:p@example.com/p', 'https://example.com/p#f']) {
    rejects(() => validateBehavior(behavior((c) => { c.url = url; })), 'CASE_URL', 'url', url);
  }
  rejects(() => validateBehavior(behavior((c) => { c.url = 'not a url'; })), 'CASE_URL', 'url');
  rejects(() => validateBehavior(behavior((c) => { c.url = 1; })), 'CASE_TYPE', 'url');
});

// --- responses entries ---

test('a_response_entry_is_a_fixture_or_a_page_with_app', () => {
  const r = validateBehavior(behavior((c) => { c.responses = { a: { page: 'a' }, f: { fixture: 'FX-LEG-FIRST-LOAD' }, g: { fixture: 'FX-LEG-A1' }, p: { page: 'p_1', app: { status: 404, headers: [['x-a', '1']] } } }; })).responses;
  assert.deepEqual(r.get('a'), { page: 'a', app: { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] } });
  assert.deepEqual(r.get('f'), { fixture: 'FX-LEG-FIRST-LOAD' });
  assert.deepEqual(r.get('g'), { fixture: 'FX-LEG-A1' });
  assert.deepEqual(r.get('p'), { page: 'p_1', app: { status: 404, headers: [['x-a', '1']] } });
});

test('a_bad_response_entry_is_rejected_with_its_member', () => {
  const bad = [
    [null, 'CASE_TYPE', 'responses.x'],
    [1, 'CASE_TYPE', 'responses.x'],
    [{ fixture: 'FX-LEG-A', page: 'a' }, 'CASE_UNKNOWN_MEMBER', 'responses.x.page'],
    [{ fixture: 1 }, 'CASE_TYPE', 'responses.x.fixture'],
    [{ fixture: 'FX-LEG-a' }, 'CASE_TYPE', 'responses.x.fixture'],
    [{ fixture: 'xFX-LEG-A' }, 'CASE_TYPE', 'responses.x.fixture'],
    [{ fixture: 'FX-LEG-A-' }, 'CASE_TYPE', 'responses.x.fixture'],
    [{ fixture: 'FX-LEG-A--B' }, 'CASE_TYPE', 'responses.x.fixture'],
    [{ page: 1 }, 'CASE_TYPE', 'responses.x.page'],
    [{ page: 'A' }, 'CASE_TYPE', 'responses.x.page'],
    [{ page: 'a!' }, 'CASE_TYPE', 'responses.x.page'],
    [{ page: 'a', body: '' }, 'CASE_UNKNOWN_MEMBER', 'responses.x.body'],
    [{ page: 'a', app: [] }, 'CASE_TYPE', 'responses.x.app'],
    [{ page: 'a', app: { status: 200, headers: [], x: 1 } }, 'CASE_UNKNOWN_MEMBER', 'responses.x.app.x'],
    [{ page: 'a', app: { status: 99, headers: [] } }, 'CASE_TYPE', 'responses.x.app.status'],
    [{ page: 'a', app: { status: 600, headers: [] } }, 'CASE_TYPE', 'responses.x.app.status'],
    [{ page: 'a', app: { status: '200', headers: [] } }, 'CASE_TYPE', 'responses.x.app.status'],
    [{ page: 'a', app: { status: 200 } }, 'CASE_TYPE', 'responses.x.app.headers'],
  ];
  for (const [entry, code, member] of bad) {
    rejects(() => validateBehavior(behavior((c) => { c.responses.x = entry; })), code, member, JSON.stringify(entry));
  }
});

test('field_pairs_are_bounded_name_value_string_pairs', () => {
  const app = (headers) => behavior((c) => { c.responses.x = { page: 'a', app: { status: 200, headers } }; });
  const pairs = Array.from({ length: 256 }, (_, i) => [`x-${i}`, '']);
  assert.equal(validateBehavior(app(pairs)).responses.get('x').app.headers.length, 256);
  const bad = [
    [...pairs, ['x', '']],
    {},
    ['x'],
    [['x']],
    [['x', 'y', 'z']],
    [[1, 'y']],
    [['x', 1]],
    [['', 'y']],
    [['x'.repeat(LIMITS.maxText + 1), 'y']],
  ];
  for (const headers of bad) rejects(() => validateBehavior(app(headers)), 'CASE_TYPE', 'responses.x.app.headers', JSON.stringify(headers).slice(0, 40));
});

// --- cache_before entries ---

test('a_cache_before_entry_keeps_its_optional_members', () => {
  const entry = { page: 'a', partition: null, url: 'https://example.com/x', vary: [['accept', null], ['x-a', 'b']], fields: [['x-f', '1']], etag: 'e', template_tag: 't' };
  assert.deepEqual(validateBehavior(behavior((c) => { c.cache_before = [entry]; })).cache_before[0], entry);
  assert.deepEqual(validateBehavior(behavior((c) => { c.cache_before = [{ page: 'a', partition: 'p' }]; })).cache_before[0], { page: 'a', partition: 'p' });
  const vary = Array.from({ length: 32 }, (_, i) => [`v${i}`, null]);
  assert.equal(validateBehavior(behavior((c) => { c.cache_before = [{ page: 'a', vary }]; })).cache_before[0].vary.length, 32);
});

test('a_bad_cache_before_entry_is_rejected_with_its_member', () => {
  const bad = [
    [{}, 'CASE_TYPE', 'cache_before[0].page'],
    [{ page: 1 }, 'CASE_TYPE', 'cache_before[0].page'],
    [{ page: 'A' }, 'CASE_TYPE', 'cache_before[0].page'],
    [{ page: 'a', size: 1 }, 'CASE_UNKNOWN_MEMBER', 'cache_before[0].size'],
    [{ page: 'a', partition: 1 }, 'CASE_TYPE', 'cache_before[0].partition'],
    [{ page: 'a', url: 'https://evil.test/' }, 'CASE_URL', 'cache_before[0].url'],
    [{ page: 'a', vary: {} }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: Array.from({ length: 33 }, (_, i) => [`v${i}`, null]) }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: ['accept'] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: [['accept']] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: [['accept', null, null]] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: [[1, null]] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: [['', null]] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', vary: [['accept', 1]] }, 'CASE_TYPE', 'cache_before[0].vary'],
    [{ page: 'a', fields: [['x']] }, 'CASE_TYPE', 'cache_before[0].fields'],
    [{ page: 'a', etag: 1 }, 'CASE_TYPE', 'cache_before[0].etag'],
    [{ page: 'a', template_tag: null }, 'CASE_TYPE', 'cache_before[0].template_tag'],
  ];
  for (const [entry, code, member] of bad) {
    rejects(() => validateBehavior(behavior((c) => { c.cache_before = [entry]; })), code, member, JSON.stringify(entry).slice(0, 60));
  }
});

// --- schedule items ---

test('the_session_steps_take_the_optional_session_member', () => {
  // README "schedule": the optional `session` of the steps that act on a session.
  for (const word of ['bind_client', 'client_ready', 'request_resource', 'page_finished', 'get_diff', 'refresh', 'destroy']) {
    assert.deepEqual(stepOf({ do: word, session: 2 }), { do: word, session: 2 });
  }
  for (const word of ['create_session', 'release', 'server_down', 'server_up', 'snapshot_cache']) {
    rejects(() => validateBehavior(withStep({ do: word, session: 1 })), 'CASE_UNKNOWN_MEMBER', 'schedule[1].session', word);
  }
});

test('each_step_word_takes_its_members_and_no_other', () => {
  const values = { session: 1, wait_ms: 0, value: 'x', valid: false, point: 'body', bytes: 0, name: 'n', for: 'session.created', timeout_ms: 0, ms: 0 };
  for (const [word, members] of Object.entries(STEP_MEMBERS)) {
    const step = { do: word, ...Object.fromEntries(members.map((m) => [m, values[m]])) };
    if (word === 'case') step.value = { page: 'a' };
    accepts(() => validateBehavior(withStep(step)), word);
    const extra = members.includes('session') ? 'ms' : 'session';
    rejects(() => validateBehavior(withStep({ ...step, [extra]: values[extra] })), 'CASE_UNKNOWN_MEMBER', `schedule[1].${extra}`, word);
  }
});

test('a_step_keeps_its_checked_members', () => {
  assert.deepEqual(stepOf({ do: 'request_resource', session: 16, wait_ms: 30_000 }), { do: 'request_resource', session: 16, wait_ms: 30_000 });
  assert.deepEqual(stepOf({ do: 'set_cookie', value: null }), { do: 'set_cookie', value: null });
  assert.deepEqual(stepOf({ do: 'set_account', value: 'rq-test-account-2' }), { do: 'set_account', value: 'rq-test-account-2' });
  assert.deepEqual(stepOf({ do: 'set_network', valid: false }), { do: 'set_network', valid: false });
  assert.deepEqual(stepOf({ do: 'hold', point: 'body', bytes: LIMITS.maxBytes }), { do: 'hold', point: 'body', bytes: LIMITS.maxBytes });
  assert.deepEqual(stepOf({ do: 'hold', point: 'request' }), { do: 'hold', point: 'request' });
  assert.deepEqual(stepOf({ do: 'cut', bytes: 7 }), { do: 'cut', bytes: 7 });
  assert.deepEqual(stepOf({ do: 'drop_field', name: 'etag' }), { do: 'drop_field', name: 'etag' });
  assert.deepEqual(stepOf({ do: 'set_field', name: 'etag', value: '' }), { do: 'set_field', name: 'etag', value: '' });
  assert.deepEqual(stepOf({ do: 'wait', for: 'cache.snapshot', timeout_ms: LIMITS.maxMs }), { do: 'wait', for: 'cache.snapshot', timeout_ms: LIMITS.maxMs });
  assert.deepEqual(stepOf({ do: 'wait', for: 'proxy.cut' }), { do: 'wait', for: 'proxy.cut' });
  assert.deepEqual(stepOf({ do: 'settle', ms: LIMITS.maxMs }), { do: 'settle', ms: LIMITS.maxMs });
});

test('a_bad_step_member_is_rejected_with_its_member', () => {
  const bad = [
    [1, 'CASE_TYPE', 'schedule[1]'],
    [{ case: 'a', do: 'release' }, 'CASE_UNKNOWN_MEMBER', 'schedule[1].do'],
    [{ case: 1 }, 'CASE_TYPE', 'schedule[1].case'],
    [{ case: 'A' }, 'CASE_TYPE', 'schedule[1].case'],
    [{ case: 'a!' }, 'CASE_TYPE', 'schedule[1].case'],
    [{}, 'CASE_STEP', 'schedule[1].do'],
    [{ do: 1 }, 'CASE_STEP', 'schedule[1].do'],
    [{ do: 'toString' }, 'CASE_STEP', 'schedule[1].do'],
    [{ do: 'destroy', session: 0 }, 'CASE_STEP', 'schedule[1].session'],
    [{ do: 'destroy', session: LIMITS.sessions + 1 }, 'CASE_STEP', 'schedule[1].session'],
    [{ do: 'destroy', session: '1' }, 'CASE_STEP', 'schedule[1].session'],
    [{ do: 'request_resource', wait_ms: -1 }, 'CASE_LIMIT', 'schedule[1].wait_ms'],
    [{ do: 'request_resource', wait_ms: LIMITS.maxWaitMs + 1 }, 'CASE_LIMIT', 'schedule[1].wait_ms'],
    [{ do: 'set_account' }, 'CASE_STEP', 'schedule[1].value'],
    [{ do: 'set_cookie', value: 1 }, 'CASE_STEP', 'schedule[1].value'],
    [{ do: 'set_network' }, 'CASE_STEP', 'schedule[1].valid'],
    [{ do: 'set_network', valid: 'true' }, 'CASE_STEP', 'schedule[1].valid'],
    [{ do: 'hold' }, 'CASE_STEP', 'schedule[1].point'],
    [{ do: 'hold', point: 'tail' }, 'CASE_STEP', 'schedule[1].point'],
    [{ do: 'hold', point: 'body' }, 'CASE_LIMIT', 'schedule[1].bytes'],
    [{ do: 'hold', point: 'body', bytes: LIMITS.maxBytes + 1 }, 'CASE_LIMIT', 'schedule[1].bytes'],
    [{ do: 'hold', point: 'request', bytes: 0 }, 'CASE_STEP', 'schedule[1].bytes'],
    [{ do: 'cut' }, 'CASE_LIMIT', 'schedule[1].bytes'],
    [{ do: 'cut', bytes: -1 }, 'CASE_LIMIT', 'schedule[1].bytes'],
    [{ do: 'drop_field' }, 'CASE_STEP', 'schedule[1].name'],
    [{ do: 'drop_field', name: '' }, 'CASE_STEP', 'schedule[1].name'],
    [{ do: 'set_field', name: '', value: 'x' }, 'CASE_STEP', 'schedule[1].name'],
    [{ do: 'set_field', name: 'x' }, 'CASE_STEP', 'schedule[1].name'],
    [{ do: 'wait' }, 'CASE_STEP', 'schedule[1].for'],
    [{ do: 'wait', for: 'session.created', timeout_ms: -1 }, 'CASE_LIMIT', 'schedule[1].timeout_ms'],
    [{ do: 'wait', for: 'session.created', timeout_ms: LIMITS.maxMs + 1 }, 'CASE_LIMIT', 'schedule[1].timeout_ms'],
    [{ do: 'settle' }, 'CASE_LIMIT', 'schedule[1].ms'],
    [{ do: 'settle', ms: -1 }, 'CASE_LIMIT', 'schedule[1].ms'],
    [{ do: 'case', value: { page: 'A' } }, 'CASE_STEP', 'schedule[1].value.page'],
    [{ do: 'case', value: { page: 1 } }, 'CASE_STEP', 'schedule[1].value.page'],
    [{ do: 'case', value: { page: 'a', status: 99 } }, 'CASE_STEP', 'schedule[1].value.status'],
    [{ do: 'case', value: { page: 'a', headers: [['x']] } }, 'CASE_TYPE', 'schedule[1].value.headers'],
    [{ do: ['release'] }, 'CASE_STEP', 'schedule[1].do'],
  ];
  for (const [step, code, member] of bad) rejects(() => validateBehavior(withStep(step)), code, member, JSON.stringify(step));
});

test('a_case_entry_must_name_a_response', () => {
  rejects(() => validateBehavior(behavior((c) => { c.schedule.push({ case: 'b' }); })), 'CASE_TYPE', 'case b');
});

// --- the full case.json ---

test('a_canonical_case_returns_its_checked_members', () => {
  const c = validateClientCase(NAME, canonical());
  assert.equal(c.kind, 'client-canonical');
  assert.deepEqual(c.rules, ['CL-01']);
  assert.deepEqual(c.rows, []);
  assert.deepEqual(c.divergences, []);
  assert.equal(c.trace, null);
  assert.equal(c.level, 'must');
  assert.equal(c.evidence, 'code-reading');
  const full = canonical();
  Object.assign(full, { rows: ['Q1', 'Q22', 'S18', 'M-01', 'M-10', 'M-16'], divergences: ['D-20'], level: 'should', rules: Array.from({ length: 256 }, () => 'CL-45') });
  const d = validateClientCase(NAME, full);
  assert.deepEqual([d.rows, d.divergences, d.level, d.rules.length], [full.rows, ['D-20'], 'should', 256]);
  const traced = { ...canonical(), evidence: 'trace', trace: { scenario: 'CS-04a', mode: 'quick' } };
  assert.deepEqual(validateClientCase(NAME, traced).trace, { scenario: 'CS-04a', mode: 'quick' });
  const standard = { ...canonical(), id: 'FX-LEG-CL-T-S', mode: 'standard' };
  assert.equal(validateClientCase('FX-LEG-CL-T-S', standard).mode, 'standard');
});

test('every_required_canonical_member_is_checked_for_presence', () => {
  for (const k of ['profile', 'mode', 'server_profile', 'rules', 'rows', 'upstream', 'evidence', 'responses', 'cache_before', 'schedule']) {
    const c = canonical();
    delete c[k];
    rejects(() => validateClientCase(NAME, c), 'CASE_MISSING_MEMBER', `case.${k}`);
  }
  // sources is checked before the presence check: its absence is a type error.
  const c = canonical();
  delete c.sources;
  rejects(() => validateClientCase(NAME, c), 'CASE_TYPE', 'sources');
});

test('a_bad_canonical_case_member_is_rejected_with_its_member', () => {
  const big = 'x'.repeat(LIMITS.maxText + 1);
  const bad = [
    [(c) => { c.id = 'FX-LEG-CL-U-Q'; }, 'CASE_TYPE', 'id'],
    [(c) => { c.kind = 'server'; }, 'CASE_TYPE', 'kind'],
    [(c) => { c.description = ''; }, 'CASE_TYPE', 'description'],
    [(c) => { c.description = 1; }, 'CASE_TYPE', 'description'],
    [(c) => { c.description = big; }, 'CASE_TYPE', 'description'],
    [(c) => { c.sources = []; }, 'CASE_TYPE', 'sources'],
    [(c) => { c.sources = 'docs:x'; }, 'CASE_TYPE', 'sources'],
    [(c) => { c.sources = ['xdocs:a']; }, 'CASE_TYPE', 'sources'],
    [(c) => { c.sources = ['docs:a b']; }, 'CASE_TYPE', 'sources'],
    [(c) => { c.sources = ['web:a']; }, 'CASE_TYPE', 'sources'],
    [(c) => { c.profile = 'legacy-android'; }, 'CASE_TYPE', 'profile'],
    [(c) => { c.mode = 'standard'; }, 'CASE_TYPE', 'mode'],
    [(c) => { c.rules = []; }, 'CASE_TYPE', 'rules'],
    [(c) => { c.rules = ['xCL-01']; }, 'CASE_TYPE', 'rules'],
    [(c) => { c.rules = ['CL-011']; }, 'CASE_TYPE', 'rules'],
    [(c) => { c.rules = [['CL-01']]; }, 'CASE_TYPE', 'rules'],
    [(c) => { c.rules = Array.from({ length: 257 }, () => 'CL-01'); }, 'CASE_TYPE', 'rules'],
    [(c) => { c.rows = ['xQ1']; }, 'CASE_TYPE', 'rows'],
    [(c) => { c.rows = ['Q1x']; }, 'CASE_TYPE', 'rows'],
    [(c) => { c.rows = ['M-00']; }, 'CASE_TYPE', 'rows'],
    [(c) => { c.rows = ['M-17']; }, 'CASE_TYPE', 'rows'],
    [(c) => { c.rows = ['S19']; }, 'CASE_TYPE', 'rows'],
    [(c) => { c.divergences = ['xD-20']; }, 'CASE_TYPE', 'divergences'],
    [(c) => { c.divergences = ['D-200']; }, 'CASE_TYPE', 'divergences'],
    [(c) => { c.upstream = { repo: UPSTREAM.repo }; }, 'CASE_TYPE', 'upstream'],
    [(c) => { c.upstream = { repo: 'x/y', sha: UPSTREAM.sha }; }, 'CASE_TYPE', 'upstream'],
    [(c) => { c.upstream = { ...UPSTREAM, ref: 'main' }; }, 'CASE_UNKNOWN_MEMBER', 'upstream.ref'],
    [(c) => { c.evidence = 'lab'; }, 'CASE_TYPE', 'evidence'],
    [(c) => { c.trace = { scenario: 'CS-04', mode: 'quick' }; }, 'CASE_TYPE', 'trace'],
    [(c) => { c.evidence = 'trace'; }, 'CASE_TYPE', 'trace'],
    [(c) => { c.evidence = 'trace'; c.trace = { scenario: 1, mode: 'quick' }; }, 'CASE_TYPE', 'trace.scenario'],
    [(c) => { c.evidence = 'trace'; c.trace = { scenario: 'xCS-04', mode: 'quick' }; }, 'CASE_TYPE', 'trace.scenario'],
    [(c) => { c.evidence = 'trace'; c.trace = { scenario: 'CS-04ab', mode: 'quick' }; }, 'CASE_TYPE', 'trace.scenario'],
    [(c) => { c.evidence = 'trace'; c.trace = { scenario: 'CS-04', mode: 'standard' }; }, 'CASE_TYPE', 'trace.mode'],
    [(c) => { c.evidence = 'trace'; c.trace = { scenario: 'CS-04', mode: 'quick' }; c.cache_before = [{ page: 'a' }]; }, 'CASE_TYPE', 'cache_before'],
    [(c) => { c.level = 'may'; }, 'CASE_TYPE', 'level'],
  ];
  for (const [change, code, member] of bad) {
    const c = canonical();
    change(c);
    rejects(() => validateClientCase(NAME, c), code, member, String(change));
  }
});

test('a_case_json_that_is_not_an_object_is_rejected', () => {
  for (const c of [null, [], 'x']) rejects(() => validateClientCase(NAME, c), 'CASE_TYPE', 'case.json');
});

test('a_directory_name_that_is_not_a_client_fixture_id_is_rejected', () => {
  for (const name of ['xFX-LEG-CL-T-Q', 'FX-LEG-CL-T-Qx', 'FX-LEG-CL-T', 'FX-LEG-T-Q']) {
    rejects(() => validateClientCase(name, { ...canonical(), id: name }), 'CASE_NAME', 'directory name', name);
  }
});

test('a_profile_case_checks_its_members', () => {
  const p = () => ({ id: `${NAME}--ANDROID`, kind: 'client', description: 'd', profile: 'legacy-android', replaces: NAME, divergences: [], sources: ['docs:x'] });
  assert.deepEqual(validateClientCase(`${NAME}--ANDROID`, p()), { kind: 'client-profile', id: `${NAME}--ANDROID`, description: 'd', profile: 'legacy-android', replaces: NAME, divergences: [], sources: ['docs:x'], notReproduced: null });
  assert.equal(validateClientCase(`${NAME}--ANDROID`, { ...p(), not_reproduced: 'U-13' }).notReproduced, 'U-13');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), profile: 'legacy-canonical' }), 'CASE_TYPE', 'profile');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), replaces: 'FX-LEG-CL-U-Q' }), 'CASE_TYPE', 'replaces');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), schedule: [] }), 'CASE_UNKNOWN_MEMBER', 'case.schedule');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), not_reproduced: 'U-11' }), 'CASE_TYPE', 'not_reproduced');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), divergences: ['D-1'] }), 'CASE_TYPE', 'divergences');
  rejects(() => validateClientCase(`${NAME}--ANDROID`, { ...p(), sources: [] }), 'CASE_TYPE', 'sources');
});
