// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The client model against the stored Android traces, its profile differences, and the
// properties of run(): the same result for equal cases in any key order, and no exception
// for a case of any shape.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { webFields } from '../src/client-android.mjs';
import { run } from '../src/client-model.mjs';
import { compareClient, projectRuns } from '../src/client-trace.mjs';
import { ENV, ROOT, recordedModes, scenarioCase, storedModes, storedRuns } from './client-helpers.mjs';

const MODES = storedModes();
const traceOf = (scenario, mode, c) => projectRuns(storedRuns(scenario, mode), { url: 'https://shop.example.com/catalog', account: c.config.account });

/**
 * The server gaps: stored modes whose lab server does what no Rivqen server profile reproduces.
 * The profile fixture of such a mode has `not_reproduced` (DL-120). CS-11: the Node.js lab server
 * adds the legacy fields to a 500 (U-13, DL-082). (CS-08 and CS-10, the Tomcat form of
 * content-type, are no gap since DL-119.)
 */
const SERVER_GAPS = new Map([['CS-11', 'U-13']]);

test('the_stored_evidence_has_every_scenario_mode_of_the_lab_record_except_the_webview_run', () => {
  const record = JSON.parse(readFileSync(join(ROOT, 'evidence/traces/client/run.json'), 'utf8'));
  const want = recordedModes();
  assert.ok(want.length > 0);
  assert.deepEqual(MODES, want);
  for (const { scenario, mode } of want) {
    const runs = record.scenarios.find((s) => s.id === scenario).modes[mode].runs;
    assert.equal(storedRuns(scenario, mode).length, runs, `${scenario} ${mode}: the stored runs of run.json`);
  }
});

test('android_model_equals_every_stored_trace_except_the_server_gaps', () => {
  for (const { scenario, mode } of MODES) {
    const c = scenarioCase(scenario, mode);
    const r = run('legacy-android', mode, c, ENV);
    assert.equal(r.ok, true, `${scenario} ${mode}: ${r.error?.message}`);
    const t = traceOf(scenario, mode, c);
    assert.equal(t.error, undefined, `${scenario} ${mode}: ${t.error}`);
    const gap = SERVER_GAPS.get(scenario) ?? null;
    assert.equal(compareClient(r.client, t.client, { notReproduced: gap }), null, `${scenario} ${mode}`);
    // A listed gap is still needed: without its item the trace differs.
    if (gap !== null) assert.match(compareClient(r.client, t.client), /"status":500,"fields":\[\]/, `${scenario} ${mode}`);
  }
});

test('canonical_model_runs_every_stored_scenario_and_differs_from_android_where_9_9_says', () => {
  for (const { scenario, mode } of MODES) {
    const c = scenarioCase(scenario, mode);
    const r = run('legacy-canonical', mode, c, ENV);
    assert.equal(r.ok, true, `${scenario} ${mode}: ${r.error?.message}`);
    const d = compareClient(r.client, traceOf(scenario, mode, c).client);
    // Section 9.9: the request fields always differ (no empty validators, no sonic-sdk-version),
    // except where no request reaches the server (CS-12: server down).
    if (scenario === 'CS-12') assert.equal(d, null);
    else assert.notEqual(d, null);
  }
});

test('canonical_cs04_sends_only_the_changed_block_and_android_every_block', () => {
  const c = scenarioCase('CS-04', 'quick');
  const message = (profile) => run(profile, 'quick', c, ENV).client.phases.flatMap((p) => p.actions).find((a) => a.do === 'message' && a.json.includes('result')).json;
  assert.deepEqual(Object.keys(JSON.parse(JSON.parse(message('legacy-canonical')).result)), ['{price}']);
  assert.deepEqual(Object.keys(JSON.parse(JSON.parse(message('legacy-android')).result)), ['{title}', '{price}', '{stock}']);
});

test('android_second_get_diff_after_a_data_update_message_gets_an_empty_object', () => {
  const r = run('legacy-android', 'standard', scenarioCase('CS-04', 'standard'), ENV);
  assert.equal(r.client.phases.at(-1).actions.at(-1).json, '{}');
});

test('android_destroy_within_the_save_delay_leaves_no_cache', () => {
  const r = run('legacy-android', 'quick', scenarioCase('CS-18', 'quick'), ENV);
  assert.deepEqual(r.client.phases.at(-1).snapshots, [{ entries: [] }]);
  const canonical = run('legacy-canonical', 'quick', scenarioCase('CS-18', 'quick'), ENV);
  assert.equal(canonical.client.phases.at(-1).snapshots[0].entries.length, 1);
});

test('android_web_view_fields_drop_repeated_names_and_the_cache_fields', () => {
  assert.deepEqual(webFields([['Cache-Offline', 'true'], ['cache-offline', 'http'], ['Etag', 'e'], ['cache-control', 'no-cache'], ['Set-Cookie', 'a=b'], ['content-type', 'x'], ['template-tag', 't']]), [['content-type', 'x'], ['template-tag', 't']]);
});

test('android_save_runs_when_the_clock_reaches_1500_ms', () => {
  const c = { ...scenarioCase('CS-01', 'quick'), schedule: [{ case: 'r1' }, { do: 'create_session' }, { do: 'settle', ms: 1499 }, { do: 'snapshot_cache' }, { do: 'settle', ms: 1 }, { do: 'snapshot_cache' }] };
  const snaps = run('legacy-android', 'quick', c, ENV).client.phases.flatMap((p) => p.snapshots);
  assert.deepEqual(snaps.map((s) => s.entries.length), [0, 1]);
});

test('android_branch_without_a_fact_row_is_not_modeled', () => {
  const c = { ...scenarioCase('CS-01', 'quick'), schedule: [{ case: 'r1' }, { do: 'create_session' }, { do: 'refresh' }] };
  const r = run('legacy-android', 'quick', c, ENV);
  assert.equal(r.ok, false);
  assert.equal(r.error.code, 'CLIENT_NOT_MODELED');
  assert.match(r.error.message, /a refresh after a first-load .*\(Q19, S16:/);
});

// --- A-15 to A-17 (S2 traces CS-20, CS-21, CS-27) ---

const S = (step, extra = {}) => ({ do: step, ...extra });
/** A first load of page a that the next session finds in the cache (the setup of CS-21 and CS-27). */
const SETUP = [{ case: 'r1' }, S('create_session'), S('bind_client'), S('wait', { for: 'proxy.response_end' }), S('client_ready'), S('request_resource'), S('settle', { ms: 2500 }), S('destroy')];
/** CS-27 up to its refresh: a template change to page c, read by the intercept, then refresh. */
const TO_REFRESH = [...SETUP, { case: 'r2' }, S('create_session'), S('bind_client'), S('wait', { for: 'proxy.response_end' }), S('settle', { ms: 300 }), S('client_ready'), S('request_resource'), S('get_diff'), S('refresh')];

function android(mode, schedule) {
  const base = scenarioCase('CS-27', mode);
  const c = { ...base, responses: { ...base.responses, e: { page: 'empty', app: base.responses.r1.app } }, schedule };
  return run('legacy-android', mode, c, ENV);
}

test('android_branches_next_to_a15_to_a17_are_not_modeled_with_their_rows', () => {
  // `held`: no cache; `withCache`: the cache of page a. Both hold the head of the empty response.
  const held = (mode, steps) => android(mode, [{ case: 'e' }, S('hold', { point: 'head' }), S('create_session'), S('bind_client'), ...steps]);
  const expect = (r, pattern) => {
    assert.equal(r.ok, false, JSON.stringify(r.client?.phases.at(-1)));
    assert.equal(r.error.code, 'CLIENT_NOT_MODELED');
    assert.match(r.error.message, pattern);
  };
  const withCache = (mode, steps) => android(mode, [...SETUP, { case: 'e' }, S('hold', { point: 'head' }), S('create_session'), S('bind_client'), ...steps]);
  expect(held('quick', [S('client_ready'), S('release')]), /an empty first-load body after the preload loadUrl.*\(Q3:/);
  expect(held('standard', [S('client_ready'), S('request_resource'), S('release')]), /empty first-load body through the bridge stream.*\(Q2, S1:/);
  expect(withCache('quick', [S('client_ready'), S('release')]), /empty body after the cached page was given as data.*\(Q12, Q13:/);
  expect(withCache('standard', [S('client_ready'), S('request_resource'), S('release')]), /empty body after the intercept.*\(S9, S10:/);
  expect(android('quick', [...SETUP, { case: 'r2' }, S('hold', { point: 'head' }), S('create_session'), S('refresh')]), /a refresh while the session runs/);
  expect(android('quick', [...SETUP, { case: 'r1' }, S('create_session'), S('refresh')]), /a refresh after a not-modified/);
  expect(android('quick', [...TO_REFRESH.slice(0, -1), S('server_down'), S('refresh')]), /a refresh without a connection/);
  expect(android('quick', [...TO_REFRESH.slice(0, -1), S('set_network', { valid: false }), S('refresh')]), /a refresh without a connection/);
  const noIntercept = TO_REFRESH.filter((s, i) => i < SETUP.length || s.do !== 'request_resource');
  expect(android('quick', noIntercept), /before the delayed save of a new page that no intercept read/);
  expect(android('quick', [...TO_REFRESH, S('request_resource')]), /an intercept after a refresh/);
  expect(android('quick', [...TO_REFRESH.slice(0, -1), { case: 'r1' }, S('refresh')]), /a template-change response to a refresh.*\(Q19, S16:/);
  expect(android('quick', [...TO_REFRESH, S('get_diff')]), /a second page message while a spaced one waits.*\(section 2.4 item 3:/);
  expect(android('quick', [...TO_REFRESH.slice(0, -1), S('destroy'), S('refresh')]), /a refresh after destroy/);
});

const REFRESH_304 = '{"code":304,"srcCode":304,"extra":{"cache-offline":"store","isReload":"true"}}';
const messagesOf = (phase) => phase.actions.filter((a) => a.do === 'message').map((a) => a.json);

test('android_after_a_refresh_page_messages_are_2_s_apart', () => {
  // The result message at get_diff (clock 2800); the refresh 304 at once: it waits until 4800.
  const r = android('quick', [
    ...TO_REFRESH, S('settle', { ms: 1999 }), S('settle', { ms: 1 }), S('settle', { ms: 2000 }),
    S('get_diff'), S('settle', { ms: 1000 }), S('get_diff'), S('settle', { ms: 999 }), S('settle', { ms: 1 }),
  ]);
  assert.equal(r.ok, true, r.error?.message);
  const phases = r.client.phases.slice(-6).map(messagesOf);
  assert.equal(phases[0].length, 1); // get_diff, refresh, settle to 4799: only the 2000 / 304 message
  assert.match(phases[0][0], /"code":304,"srcCode":2000/);
  assert.deepEqual(phases[1], [REFRESH_304]); // 4800
  assert.deepEqual(phases[2], []);
  assert.deepEqual(phases[3], [REFRESH_304]); // get_diff at 6800: exactly 2 s later, at once
  assert.deepEqual(phases[4], []); // get_diff at 7800 waits until 8800 (2 s after the last message), not 9800
  assert.deepEqual(phases[5], [REFRESH_304]);
});

const PAGE_A = 'a974949d';
const PAGE_C = '3d863596';
const savedPages = (r) => r.client.phases.flatMap((p) => p.snapshots).map((s) => s.entries[0].page_sha256.slice(0, 8));

test('android_refresh_moves_the_save_to_1500_ms_after_the_stream_close', () => {
  // Body end at 2500 (delayed save due 4000); the intercept reads page c at 2800; the refresh at
  // 3300 cancels the delayed save, and the stream-close save runs at 2800 + 1500 = 4300.
  const r = android('quick', [...TO_REFRESH.slice(0, -1), S('settle', { ms: 500 }), S('refresh'), S('settle', { ms: 999 }), S('snapshot_cache'), S('settle', { ms: 1 }), S('snapshot_cache')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(savedPages(r), [PAGE_A, PAGE_C]);
});

test('android_destroy_after_a_refresh_keeps_the_stream_close_save', () => {
  const r = android('quick', [...TO_REFRESH, S('destroy'), S('settle', { ms: 1500 }), S('snapshot_cache')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(savedPages(r), [PAGE_C]);
  // Without the refresh, the destroy cancels the delayed save (A-13): page a stays.
  assert.deepEqual(savedPages(android('quick', [...TO_REFRESH.slice(0, -1), S('destroy'), S('settle', { ms: 2500 }), S('snapshot_cache')])), [PAGE_A]);
});

test('android_refresh_after_the_delayed_save_needs_no_intercept', () => {
  const noIntercept = TO_REFRESH.filter((s, i) => i < SETUP.length || s.do !== 'request_resource');
  const r = android('quick', [...noIntercept.slice(0, -1), S('settle', { ms: 2000 }), S('refresh'), S('snapshot_cache'), S('destroy')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(savedPages(r), [PAGE_C]);
  assert.deepEqual(r.client.phases.at(-1).results, [{ session: 2, src_code: 304, final_code: 304 }]);
});

test('android_refresh_resets_the_codes_until_its_response', () => {
  const r = android('quick', [...TO_REFRESH.slice(0, -1), S('hold', { point: 'head' }), S('refresh'), S('destroy')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(r.client.phases.at(-1).results, [{ session: 2, src_code: -1, final_code: -1 }]);
  const again = android('quick', [...TO_REFRESH.slice(0, -1), S('hold', { point: 'head' }), S('refresh'), S('refresh')]);
  assert.match(again.error.message, /a refresh while the session runs/);
});

test('android_page_messages_are_not_spaced_without_a_refresh', () => {
  // CS-03 order (304 with a cache), and two get_diff calls at one clock: two messages at once.
  const r = android('quick', [...SETUP, { case: 'r1' }, S('create_session'), S('bind_client'), S('client_ready'), S('get_diff'), S('get_diff')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.equal(messagesOf(r.client.phases.at(-1)).length, 2);
});

test('android_first_message_after_a_refresh_is_not_spaced', () => {
  // The cache is saved at 1500; the template change and the refresh run at 1500, before any message.
  const setup = SETUP.map((s) => (s.do === 'settle' ? S('settle', { ms: 1500 }) : s));
  const r = android('quick', [...setup, { case: 'r2' }, S('create_session'), S('bind_client'), S('wait', { for: 'proxy.response_end' }), S('client_ready'), S('request_resource'), S('refresh'), S('get_diff')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(messagesOf(r.client.phases.at(-1)), [REFRESH_304]);
});

test('android_empty_first_load_in_standard_notifies_a_waiting_page', () => {
  // S1 "notify": a get_diff before the end of the empty body gets 1000 / 1000 at once.
  const r = android('standard', [{ case: 'e' }, S('hold', { point: 'head' }), S('create_session'), S('get_diff'), S('release')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(messagesOf(r.client.phases.at(-1)).map((m) => JSON.parse(m).code), [1000]);
});

test('android_cut_at_0_body_bytes_through_the_bridge_stream_is_q22_not_an_empty_body', () => {
  const r = android('standard', [{ case: 'e' }, S('cut', { bytes: 0 }), S('hold', { point: 'head' }), S('create_session'), S('bind_client'), S('client_ready'), S('request_resource'), S('release'), S('destroy')]);
  assert.equal(r.ok, true, r.error?.message);
  assert.deepEqual(r.client.phases.at(-1).results, [{ session: 1, src_code: 1000, final_code: 1000 }]);
});

test('run_rejects_a_mode_that_differs_from_the_case', () => {
  const r = run('legacy-canonical', 'standard', scenarioCase('CS-01', 'quick'), ENV);
  assert.deepEqual(r.error.code, 'CASE_MODE');
});

test('run_throws_for_an_unknown_profile_or_mode', () => {
  assert.throws(() => run('legacy-ios', 'quick', {}, ENV), { code: 'LEGACY_UNKNOWN_PROFILE' });
  assert.throws(() => run('legacy-canonical', 'fast', {}, ENV), { code: 'LEGACY_UNKNOWN_MODE' });
});

test('a_wait_for_an_event_that_does_not_occur_is_a_case_error', () => {
  const c = { ...scenarioCase('CS-01', 'quick'), schedule: [{ case: 'r1' }, { do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'wait', for: 'proxy.response_end' }] };
  assert.equal(run('legacy-canonical', 'quick', c, ENV).error.code, 'CASE_WAIT');
});

test('a_request_before_the_first_case_entry_is_a_case_error', () => {
  const c = { ...scenarioCase('CS-01', 'quick'), schedule: [{ do: 'create_session' }] };
  assert.equal(run('legacy-canonical', 'quick', c, ENV).error.code, 'CASE_NO_RESPONSE');
});

test('release_without_a_hold_and_a_second_hold_are_case_errors', () => {
  const base = scenarioCase('CS-01', 'quick');
  assert.equal(run('legacy-canonical', 'quick', { ...base, schedule: [{ do: 'release' }] }, ENV).error.code, 'CASE_HOLD');
  assert.equal(run('legacy-canonical', 'quick', { ...base, schedule: [{ do: 'hold', point: 'head' }, { do: 'hold', point: 'head' }] }, ENV).error.code, 'CASE_HOLD');
});

test('a_step_on_a_session_that_was_not_opened_is_a_case_error', () => {
  const base = scenarioCase('CS-01', 'quick');
  assert.equal(run('legacy-canonical', 'quick', { ...base, schedule: [{ do: 'bind_client' }] }, ENV).error.code, 'CASE_NO_SESSION');
  assert.equal(run('legacy-canonical', 'quick', { ...base, schedule: [{ case: 'r1' }, { do: 'create_session' }, { do: 'get_diff', session: 2 }] }, ENV).error.code, 'CASE_NO_SESSION');
});

test('a_missing_page_is_a_case_error', () => {
  const base = scenarioCase('CS-01', 'quick');
  const c = { ...base, responses: { r1: { page: 'nosuchpage' } } };
  assert.equal(run('legacy-canonical', 'quick', c, ENV).error.code, 'CASE_PAGE');
});

// --- properties (seeded, deterministic) ---

function prng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The same value with the members of every object in a random order. */
function shuffleKeys(value, rand) {
  if (Array.isArray(value)) return value.map((v) => shuffleKeys(v, rand));
  if (value === null || typeof value !== 'object') return value;
  const keys = Object.keys(value);
  for (let i = keys.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [keys[i], keys[j]] = [keys[j], keys[i]];
  }
  return Object.fromEntries(keys.map((k) => [k, shuffleKeys(value[k], rand)]));
}

test('property_the_result_is_the_same_for_equal_cases_in_any_key_order', () => {
  const rand = prng(20261107);
  for (const { scenario, mode } of MODES) {
    const c = scenarioCase(scenario, mode);
    for (const profile of ['legacy-canonical', 'legacy-android']) {
      const want = JSON.stringify(run(profile, mode, c, ENV));
      for (let i = 0; i < 3; i++) assert.equal(JSON.stringify(run(profile, mode, shuffleKeys(c, rand), ENV)), want, `${scenario} ${mode} ${profile}`);
    }
  }
});

const ATOMS = [null, true, false, 0, -1, 1.5, 2 ** 40, '', 'x', 'quick', 'create_session', 'a', [], {}, [['a', 'b']], { page: 'a' }, { do: 'settle', ms: 1 }, { case: 'r1' }];

function randomValue(rand, depth) {
  const r = rand();
  if (depth <= 0 || r < 0.5) return ATOMS[Math.floor(rand() * ATOMS.length)];
  if (r < 0.75) return Array.from({ length: Math.floor(rand() * 4) }, () => randomValue(rand, depth - 1));
  const keys = ['mode', 'do', 'case', 'page', 'app', 'status', 'headers', 'session', 'point', 'bytes', 'value', 'ms', 'for', 'fixture', 'x'];
  return Object.fromEntries(Array.from({ length: Math.floor(rand() * 4) }, () => [keys[Math.floor(rand() * keys.length)], randomValue(rand, depth - 1)]));
}

/** A valid case with one member or one step replaced by a random value. */
function mutate(c, rand) {
  const out = structuredClone(c);
  const where = rand();
  if (where < 0.3) {
    const keys = Object.keys(out);
    out[keys[Math.floor(rand() * keys.length)]] = randomValue(rand, 3);
  } else if (where < 0.8 && out.schedule.length > 0) {
    const i = Math.floor(rand() * out.schedule.length);
    if (rand() < 0.5) out.schedule[i] = randomValue(rand, 2);
    else out.schedule.splice(i, 1);
  } else {
    out.schedule.splice(Math.floor(rand() * out.schedule.length), 0, randomValue(rand, 2));
  }
  return out;
}

test('property_the_model_never_throws_for_a_case_of_any_shape', () => {
  const rand = prng(7);
  for (let i = 0; i < 300; i++) {
    for (const profile of ['legacy-canonical', 'legacy-android']) {
      const r = run(profile, 'quick', randomValue(rand, 4), ENV);
      assert.equal(typeof r.ok, 'boolean');
      if (!r.ok) assert.match(r.error.code, /^(CASE|CLIENT)_/);
    }
  }
  for (const { scenario, mode } of MODES) {
    const c = scenarioCase(scenario, mode);
    for (let i = 0; i < 20; i++) {
      for (const profile of ['legacy-canonical', 'legacy-android']) {
        const r = run(profile, mode, mutate(c, rand), ENV);
        assert.equal(typeof r.ok, 'boolean', `${scenario} ${mode}`);
        if (!r.ok) assert.match(r.error.code, /^(CASE|CLIENT)_/);
      }
    }
  }
});
