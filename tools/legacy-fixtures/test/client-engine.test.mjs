// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The step engine (src/client-engine.mjs) through run(): the `wait` step for each event of
// CONTRACT section 5 that the model produces, the response entries that name a server fixture,
// and the session that a step names.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { run } from '../src/client-model.mjs';
import { ENV } from './client-helpers.mjs';

const APP = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['set-cookie', 'a=b']] };
const LOAD = [{ do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }];
const cc = (schedule, { mode = 'quick', responses = { a: { page: 'a', app: APP }, b: { page: 'b', app: APP } }, config } = {}) => ({ mode, server_profile: 'legacy-canonical', responses, cache_before: [], schedule, ...(config ? { config } : {}) });
const outcome = (c, env = ENV) => run('legacy-canonical', c.mode, c, env);

test('a_wait_passes_for_each_event_that_occurred_after_the_last_create_session', () => {
  const faults = [{ case: 'a' }, { do: 'hold', point: 'head' }, { do: 'drop_field', name: 'etag' }, { do: 'set_field', name: 'x-a', value: '1' }, { do: 'set_body_member', name: 'x', value: 'y' }, { do: 'create_session' }, ...LOAD, { do: 'release' }, { do: 'settle', ms: 2000 }, { do: 'snapshot_cache' }, { do: 'destroy' }];
  const cut = [{ case: 'a' }, { do: 'cut', bytes: 10 }, { do: 'create_session' }, ...LOAD, { do: 'settle', ms: 2000 }];
  const update = [{ case: 'a' }, { do: 'create_session' }, ...LOAD, { do: 'settle', ms: 2000 }, { do: 'destroy' }, { case: 'b' }, { do: 'create_session' }, ...LOAD, { do: 'get_diff' }, { do: 'settle', ms: 2000 }];
  const first = [{ case: 'a' }, { do: 'create_session' }, ...LOAD, { do: 'settle', ms: 2000 }];
  const groups = [
    ['quick', first, ['client.load_data']],
    ['quick', faults, ['session.created', 'runtime.cookie_get', 'runtime.cookie_set', 'proxy.request', 'proxy.hold', 'proxy.drop_field', 'proxy.set_field', 'proxy.set_body_member', 'proxy.release', 'proxy.response_head', 'proxy.response_end', 'client.load_url', 'client.intercept', 'cache.snapshot', 'session.result']],
    ['quick', cut, ['proxy.cut']],
    ['standard', update, ['session.message']],
  ];
  for (const [mode, schedule, events] of groups) {
    for (const event of events) {
      const r = outcome(cc([...schedule, { do: 'wait', for: event }], { mode }));
      assert.equal(r.ok, true, `${event}: ${r.error?.message}`);
    }
  }
});

test('a_wait_does_not_see_events_before_the_last_create_session', () => {
  const r = outcome(cc([{ case: 'a' }, { do: 'snapshot_cache' }, { do: 'create_session' }, { do: 'wait', for: 'cache.snapshot' }]));
  assert.equal(r.error.code, 'CASE_WAIT');
  assert.match(r.error.message, /^step 3: wait for cache\.snapshot/);
});

test('a_fixture_response_entry_answers_with_the_page_and_app_of_that_server_fixture', () => {
  const schedule = [{ case: 'f' }, { do: 'create_session' }, ...LOAD, { do: 'settle', ms: 2000 }];
  const env = { ...ENV, fixture: (id) => (id === 'FX-LEG-PAGE-A' ? { page: 'a', app: APP } : null) };
  const byFixture = outcome(cc(schedule, { responses: { f: { fixture: 'FX-LEG-PAGE-A' } } }), env);
  assert.equal(byFixture.ok, true);
  assert.deepEqual(byFixture.client, outcome(cc(schedule, { responses: { f: { page: 'a', app: APP } } })).client);
  const unknown = outcome(cc(schedule, { responses: { f: { fixture: 'FX-LEG-OTHER' } } }), env);
  assert.deepEqual([unknown.error.code, unknown.error.message], ['CASE_FIXTURE', 'responses: server fixture FX-LEG-OTHER is not a canonical server fixture']);
  assert.equal(outcome(cc(schedule, { responses: { f: { fixture: 'FX-LEG-PAGE-A' } } }), { page: ENV.page }).error.code, 'CASE_FIXTURE');
  assert.equal(outcome(cc(schedule, { responses: { f: { page: 'a' } } }), { fixture: env.fixture }).error.code, 'CASE_PAGE');
});

test('a_step_acts_on_the_session_that_it_names', () => {
  const two = [{ case: 'a' }, { do: 'create_session' }, { do: 'set_account', value: 'rq-test-account-2' }, { do: 'create_session' }];
  const r = outcome(cc([...two, { do: 'destroy', session: 1 }]));
  assert.deepEqual(r.client.phases[0].results.map((x) => x.session), [1]);
  const none = outcome(cc([...two, { do: 'destroy', session: 3 }]));
  assert.deepEqual([none.error.code, none.error.message], ['CASE_NO_SESSION', 'step 4: session 3 was not opened']);
});

test('cl06_cl34_steps_on_a_destroyed_or_stopped_session_send_and_show_nothing', () => {
  const later = [{ do: 'bind_client', session: 1 }, { do: 'client_ready', session: 1 }, { do: 'request_resource', session: 1 }, { do: 'get_diff', session: 1 }, { do: 'page_finished', session: 1 }, { do: 'refresh', session: 1 }, { do: 'destroy', session: 1 }, { do: 'settle', ms: 3000 }];
  const silent = { requests: [], cookies: [], actions: [], intercepts: [{ session: 1, null: true }], snapshots: [] };
  for (const mode of ['quick', 'standard']) {
    // Destroyed: a later step has no effect; the intercept gets null (CL-34).
    const destroyed = outcome(cc([{ case: 'a' }, { do: 'create_session' }, { do: 'destroy' }, { do: 'settle', ms: 10 }, ...later], { mode }));
    assert.deepEqual(destroyed.client.phases.at(-1), { steps: [4, 11], ...silent, results: [] }, mode);
    // Stopped by an account change (CL-06): the same, and the destroy reports the result once.
    const stopped = outcome(cc([{ case: 'a' }, { do: 'create_session' }, { do: 'set_account', value: 'rq-test-account-2' }, { do: 'settle', ms: 10 }, ...later], { mode }));
    const last = stopped.client.phases.at(-1);
    assert.deepEqual({ ...last, results: [] }, { steps: [4, 11], ...silent, results: [] }, mode);
    assert.equal(last.results.length, 1, mode);
  }
});

test('a_case_whose_mode_is_not_the_run_mode_is_not_ok', () => {
  const r = run('legacy-canonical', 'standard', cc([{ case: 'a' }, { do: 'create_session' }]), ENV);
  assert.equal(r.ok, false);
  assert.equal(r.error.code, 'CASE_MODE');
});

test('an_exception_that_is_not_a_case_error_propagates', () => {
  // A defect of the environment (here: the page source) is not a case error: run() does not hide it.
  const env = { page: () => { throw new RangeError('page source failed'); }, fixture: () => null };
  assert.throws(() => outcome(cc([{ case: 'a' }, { do: 'create_session' }]), env), RangeError);
});
