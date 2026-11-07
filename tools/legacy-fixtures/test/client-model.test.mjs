// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The client model against the stored Android traces, its profile differences, and the
// properties of run(): the same result for equal cases in any key order, and no exception
// for a case of any shape.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { webFields } from '../src/client-android.mjs';
import { run } from '../src/client-model.mjs';
import { compareClient, projectRuns } from '../src/client-trace.mjs';
import { ENV, scenarioCase, storedModes, storedRuns } from './client-helpers.mjs';

const MODES = storedModes();
const traceOf = (scenario, mode, c) => projectRuns(storedRuns(scenario, mode), { url: 'https://shop.example.com/catalog', account: c.config.account });

/**
 * Stored modes that the server models cannot reproduce (questions to the architect, T-04 report):
 * CS-08 and CS-10 come from the Java lab server, which writes `text/html;charset=utf-8`;
 * CS-11 gets legacy fields on a 500 from the Node lab server (U-13, not reproduced, DL-082).
 */
const SERVER_GAPS = new Map([
  ['CS-08', /content-type","text\/html; charset=utf-8/],
  ['CS-10', /content-type","text\/html; charset=utf-8/],
  ['CS-11', /"status":500,"fields":\[\]/],
]);

test('the stored evidence has the 38 scenario modes of S1 (CS-19 excepted)', () => {
  assert.equal(MODES.length, 38);
});

test('android_model_equals_every_stored_trace_except_the_server_gaps', () => {
  for (const { scenario, mode } of MODES) {
    const c = scenarioCase(scenario, mode);
    const r = run('legacy-android', mode, c, ENV);
    assert.equal(r.ok, true, `${scenario} ${mode}: ${r.error?.message}`);
    const t = traceOf(scenario, mode, c);
    assert.equal(t.error, undefined, `${scenario} ${mode}: ${t.error}`);
    const d = compareClient(r.client, t.client);
    if (SERVER_GAPS.has(scenario)) assert.match(d ?? '', SERVER_GAPS.get(scenario), `${scenario} ${mode}`);
    else assert.equal(d, null, `${scenario} ${mode}: ${d}`);
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
  assert.match(r.error.message, /Q19, S16/);
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
