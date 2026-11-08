// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The conductor steps `set_body_member` and `case` (tools/upstream-lab/client/CONTRACT.md
// section 3, S2, H-126): the case check, the engine, and the projection of the work file that
// a failed rebuild leaves (CS-28).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CANONICAL_PROFILE } from '../src/client-canonical.mjs';
import { validateBehavior } from '../src/client-case.mjs';
import { Sim } from '../src/client-engine.mjs';
import { run } from '../src/client-model.mjs';
import { parseRun, projectRun } from '../src/client-trace.mjs';
import { ENV, scenarioCase } from './client-helpers.mjs';

const ZEROS = '0'.repeat(40);
const FIRST = [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'settle', ms: 100 }, { do: 'destroy' }];
const SECOND = [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'get_diff' }, { do: 'settle', ms: 100 }, { do: 'snapshot_cache' }, { do: 'destroy' }];

const cc = (schedule, { mode = 'quick', server_profile = 'legacy-node' } = {}) => ({ mode, server_profile, responses: { a: { page: 'a' }, b: { page: 'b' } }, cache_before: [], schedule });

/** Run under the canonical profile and keep the engine for its fault log. */
function sim(c) {
  const s = new Sim(CANONICAL_PROFILE, validateBehavior(c), ENV);
  return { client: s.run(), edits: s.bodyEdits };
}

const errorOf = (c, profile = 'legacy-canonical') => {
  const r = run(profile, c.mode, c, ENV);
  return r.ok ? null : r.error;
};
const results = (client) => client.phases.flatMap((p) => p.results);
const requests = (client) => client.phases.flatMap((p) => p.requests);
const lastSnapshot = (client) => client.phases.flatMap((p) => p.snapshots).at(-1).entries;

// --- the case check ---

test('set_body_member_and_case_are_step_words_with_the_lab_members', () => {
  const c = cc([{ case: 'a' }, { do: 'set_body_member', name: 'html-sha1', value: '' }, { do: 'case', value: { page: 'b', status: 500, headers: [['x-a', '1']] } }, { do: 'create_session' }]);
  const b = validateBehavior(c);
  assert.deepEqual(b.schedule[1], { do: 'set_body_member', name: 'html-sha1', value: '' });
  assert.deepEqual(b.schedule[2], { do: 'case', response: { page: 'b', app: { status: 500, headers: [['x-a', '1']] } } });
  // The defaults of a `responses` entry.
  assert.deepEqual(validateBehavior(cc([{ do: 'case', value: { page: 'a' } }])).schedule[0].response.app, { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });
});

test('set_body_member_and_case_steps_with_a_wrong_member_are_case_errors', () => {
  const bad = [
    [{ do: 'set_body_member', name: '', value: 'x' }, 'CASE_STEP'],
    [{ do: 'set_body_member', name: 'n' }, 'CASE_STEP'],
    [{ do: 'set_body_member', name: 'n', value: 1 }, 'CASE_STEP'],
    [{ do: 'set_body_member', name: 'n', value: 'x', bytes: 1 }, 'CASE_UNKNOWN_MEMBER'],
    [{ do: 'case' }, 'CASE_TYPE'],
    [{ do: 'case', value: 'a' }, 'CASE_TYPE'],
    [{ do: 'case', value: { page: 'A b' } }, 'CASE_STEP'],
    [{ do: 'case', value: { page: 'a', status: 99 } }, 'CASE_STEP'],
    [{ do: 'case', value: { page: 'a', status: 600 } }, 'CASE_STEP'],
    [{ do: 'case', value: { page: 'a', headers: [['x']] } }, 'CASE_TYPE'],
    [{ do: 'case', value: { page: 'a', app: {} } }, 'CASE_UNKNOWN_MEMBER'],
    [{ do: 'case', value: { page: 'a' }, session: 1 }, 'CASE_UNKNOWN_MEMBER'],
  ];
  for (const [step, code] of bad) assert.equal(errorOf(cc([{ case: 'a' }, step])).code, code, JSON.stringify(step));
});

// --- set_body_member in the engine ---

test('set_body_member_edits_the_json_body_of_the_next_exchange_only', () => {
  // Armed before the first load: the HTML page is not JSON, so the next exchange is unchanged and
  // the later data update of the second session is not edited.
  const r = sim(cc([{ case: 'a' }, { do: 'set_body_member', name: 'html-sha1', value: ZEROS }, ...FIRST, { case: 'b' }, ...SECOND]));
  assert.deepEqual(r.edits, [{ exchange: 1, name: 'html-sha1', value: ZEROS, applied: false, reason: 'body is not JSON' }]);
  assert.deepEqual(results(r.client).at(-1), { session: 2, src_code: 200, final_code: 304 });
  assert.equal(lastSnapshot(r.client).length, 1);
});

test('set_body_member_with_a_wrong_html_sha1_fails_the_canonical_rebuild_check', () => {
  // CL-29 (CS-28): a normal load, the entry removed, -1 / -1.
  const r = sim(cc([{ case: 'a' }, ...FIRST, { case: 'b' }, { do: 'set_body_member', name: 'html-sha1', value: ZEROS }, ...SECOND]));
  assert.deepEqual(r.edits, [{ exchange: 2, name: 'html-sha1', value: ZEROS, applied: true }]);
  assert.deepEqual(results(r.client).at(-1), { session: 2, src_code: -1, final_code: -1 });
  assert.deepEqual(lastSnapshot(r.client), []);
  assert.deepEqual(r.client.phases.flatMap((p) => p.actions).filter((a) => a.session === 2).map((a) => a.do), ['load_url']);
});

test('set_body_member_on_a_304_is_not_applied_with_the_reason', () => {
  const r = sim(cc([{ case: 'a' }, ...FIRST, { do: 'set_body_member', name: 'x', value: 'y' }, ...SECOND]));
  assert.equal(requests(r.client).at(-1).response.status, 304);
  assert.deepEqual(r.edits, [{ exchange: 2, name: 'x', value: 'y', applied: false, reason: 'empty body' }]);
});

test('set_body_member_delivers_the_edited_body_with_a_content_length_and_no_content_coding', () => {
  // The Android record of CS-28 Standard keeps the delivered header lines (A-19).
  const r = run('legacy-android', 'standard', scenarioCase('CS-28', 'standard'), ENV);
  const fields = r.client.phases.flatMap((p) => p.snapshots).at(-1).entries[0].fields;
  assert.deepEqual(fields.filter(([n]) => n === 'content-length'), [['content-length', '334']]);
  assert.deepEqual(fields.filter(([n]) => n === 'content-encoding' || n === 'transfer-encoding'), []);
});

test('server_down_clears_an_armed_set_body_member', () => {
  const r = sim(cc([{ case: 'a' }, ...FIRST, { case: 'b' }, { do: 'set_body_member', name: 'html-sha1', value: ZEROS }, { do: 'server_down' }, { do: 'server_up' }, ...SECOND]));
  assert.deepEqual(r.edits, []);
  assert.deepEqual(results(r.client).at(-1), { session: 2, src_code: 200, final_code: 304 });
});

test('a_wait_for_proxy_set_body_member_follows_the_edit', () => {
  const c = cc([{ case: 'a' }, { do: 'set_body_member', name: 'n', value: 'v' }, { do: 'create_session' }, { do: 'wait', for: 'proxy.set_body_member' }]);
  assert.equal(errorOf(c), null);
  assert.equal(errorOf(cc([{ case: 'a' }, { do: 'create_session' }, { do: 'wait', for: 'proxy.set_body_member' }])).code, 'CASE_WAIT');
});

// --- the case step in the engine ---

test('a_case_step_changes_the_application_response_of_later_exchanges', () => {
  // The first load is page a; the case step posts page b; the second session gets a data update.
  const r = sim(cc([{ case: 'a' }, ...FIRST, { do: 'case', value: { page: 'b' } }, ...SECOND]));
  assert.deepEqual(results(r.client).map((x) => x.src_code), [1000, 200]);
  const status = sim(cc([{ case: 'a' }, ...FIRST, { do: 'case', value: { page: 'a', status: 500 } }, ...SECOND]));
  assert.equal(requests(status.client).at(-1).response.status, 500);
});

test('a_case_step_is_a_numbered_step_and_may_come_before_the_first_request', () => {
  const r = sim(cc([{ do: 'case', value: { page: 'a' } }, ...FIRST]));
  assert.deepEqual(r.client.phases[0].steps, [1, 6]);
  assert.deepEqual(results(r.client), [{ session: 1, src_code: 1000, final_code: 304 }]);
});

test('a_case_step_while_an_exchange_is_held_at_its_request_is_a_case_error', () => {
  const held = (point) => cc([{ case: 'a' }, { do: 'hold', point }, { do: 'create_session' }, { do: 'case', value: { page: 'b' } }, { do: 'release' }]);
  assert.equal(errorOf(held('request')).code, 'CASE_HOLD');
  // A head hold: the lab has the answer already, so the case step does not change it.
  assert.equal(errorOf(held('head')), null);
  // An armed hold that no exchange reached yet does not block the case step.
  assert.equal(errorOf(cc([{ case: 'a' }, { do: 'hold', point: 'request' }, { do: 'case', value: { page: 'b' } }, { do: 'create_session' }, { do: 'release' }])), null);
});

// --- the work file of a failed rebuild in the projection ---

const CTX = { url: 'https://shop.example.com/catalog', account: 'rq-test-account' };
const lines = (...events) => Buffer.from(`${events.map((e) => JSON.stringify(e)).join('\n')}\n`);
const step = (seq, s) => ({ ev: 'step.start', src: 'conductor', seq, step: { ...s, seq } });
const snapshotRun = (files) => parseRun(lines(step(1, { do: 'create_session' }), { ev: 'session.created', src: 'harness', hseq: 1, session: 1, session_id: 'sid' }, step(2, { do: 'snapshot_cache' }), { ev: 'cache.snapshot', src: 'harness', hseq: 2, files }));

test('the_html_tmp_work_file_is_not_part_of_a_cache_record', () => {
  assert.deepEqual(projectRun(snapshotRun([{ name: 'sid.html.tmp', size: 1, sha256: 'x' }]), CTX).phases[0].snapshots, [{ entries: [] }]);
  const [entry] = projectRun(snapshotRun([{ name: 'sid.data', size: 1, sha256: 'd' }, { name: 'sid.html.tmp', size: 1, sha256: 'x' }]), CTX).phases[0].snapshots[0].entries;
  assert.deepEqual([entry.page_sha256, entry.data_sha256], [null, 'd']);
});

test('an_html_tmp_work_file_of_no_session_is_a_projection_error', () => {
  assert.throws(() => projectRun(snapshotRun([{ name: 'other.html.tmp', size: 1, sha256: 'x' }]), CTX), { code: 'TRACE_SNAPSHOT' });
  assert.throws(() => projectRun(snapshotRun([{ name: 'sid.tpl.tmp', size: 1, sha256: 'x' }]), CTX), { code: 'TRACE_SNAPSHOT' });
});
